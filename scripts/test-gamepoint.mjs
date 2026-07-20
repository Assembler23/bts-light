// Testet die Satz-/Matchball-Erkennung (src/io/gamePoint.mjs, Plan 16) — das
// echte Modul, das die Felderübersicht nutzt.
import { gamePointKind, setDecided, setsToWin } from "../src/io/gamePoint.mjs";

let failures = 0;
function eq(name, got, want) {
  if (got !== want) {
    console.error(`✗ ${name}: erwartet ${want}, war ${got}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

// Standard-Zählformat 21 (Cap 30), Best-of-3.
const F = { best_of: 3, target_score: 21, cap_score: 30, match_id: 7 };
const c = (sets) => Object.assign({ sets }, F);

// setsToWin
eq("setsToWin(3)", setsToWin(3), 2);
eq("setsToWin(5)", setsToWin(5), 3);
eq("setsToWin(1)", setsToWin(1), 1);
eq("setsToWin(0 → Default 3)", setsToWin(0), 2);

// setDecided
eq("21:19 entschieden", setDecided(21, 19, 21, 30), true);
eq("21:20 NICHT (nur 1 vor)", setDecided(21, 20, 21, 30), false);
eq("30:29 Cap erreicht", setDecided(30, 29, 21, 30), true);
eq("20:19 nicht entschieden", setDecided(20, 19, 21, 30), false);

// Kein Ball
eq("0:0 kein Ball", gamePointKind(c([[0, 0]])), null);
eq("15:10 kein Ball", gamePointKind(c([[15, 10]])), null);
eq("kein Match", gamePointKind({ ...c([[20, 5]]), match_id: 0 }), null);
eq("leere Sätze", gamePointKind(c([])), null);

// Satzball im 1. Satz (führt, aber Satz gewinnt Match noch nicht)
eq("20:15 → Satzball", gamePointKind(c([[20, 15]])), "set");
eq("20:20 kein Ball (kein Vorsprung)", gamePointKind(c([[20, 20]])), null);
eq("20:19 → Satzball (1 vor, ≥ target-1)", gamePointKind(c([[20, 19]])), "set");

// Matchball: Führender hat schon 1 Satz und ist am Satzball im 2. Satz.
eq(
  "Satz 1 gewonnen + 20:15 → Matchball",
  gamePointKind(
    c([
      [21, 18],
      [20, 15],
    ]),
  ),
  "match",
);
// Gegner hat den 1. Satz → im 2. Satz nur Satzball (Ausgleich, kein Matchende).
eq(
  "Gegner führt nach Sätzen → Satzball",
  gamePointKind(
    c([
      [18, 21],
      [20, 15],
    ]),
  ),
  "set",
);
// Bereits entschiedener laufender Satz zeigt keinen Ball.
eq("21:19 (Satz vorbei) → null", gamePointKind(c([[21, 19]])), null);
// Cap-Satzball: 29:28 ist ein Ball (bei 30 Cap gewinnt der nächste Punkt).
eq("29:28 → Satzball (Cap-Nähe)", gamePointKind(c([[29, 28]])), "set");
// Cap-Patt 29:29: nächster Punkt entscheidet für BEIDE → Ball trotz 0 Vorsprung.
eq("29:29 (Satz 1) → Satzball", gamePointKind(c([[29, 29]])), "set");
eq(
  "29:29 mit 1 Satzvorsprung → Matchball (eine Seite kann beenden)",
  gamePointKind(
    c([
      [21, 10],
      [29, 29],
    ]),
  ),
  "match",
);
eq(
  "29:29 mit Gegner-Satzvorsprung → Matchball (Gegner kann beenden)",
  gamePointKind(
    c([
      [10, 21],
      [29, 29],
    ]),
  ),
  "match",
);
// Decider-Matchball: 1:1 Sätze, 20:18 im 3. Satz.
eq(
  "1:1 Sätze + 20:18 → Matchball",
  gamePointKind(
    c([
      [21, 10],
      [10, 21],
      [20, 18],
    ]),
  ),
  "match",
);

if (failures > 0) {
  console.error(`\n✗ Game-Point-Test: ${failures} Fehler.`);
  process.exit(1);
}
console.log("\n✓ Game-Point-Test: Satz-/Matchball-Erkennung ok.");
