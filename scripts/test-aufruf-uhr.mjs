// Testet die Aufruf-Uhr des Tablets (src/io/aufrufUhr.mjs) — das echte
// Modul, dessen Inline-Kopie `tablet.html` trägt.
//
// Warum das wichtig ist: Die Uhr steht während der Seitenwahl neben dem
// Feldnamen. Zeigt sie zu früh „Letzter Aufruf", rennt der Bediener zur
// Turnierleitung; zeigt sie trotz laufendem Spiel weiter, stehen zwei Uhren
// nebeneinander und keiner weiß, welche gilt.
import { aufrufUhr, feldAusHealth } from "../src/io/aufrufUhr.mjs";

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

const ct = { enabled: true, secondCallMinutes: 2, thirdCallMinutes: 4 };
const T0 = 1_700_000_000_000;
const min = (m) => T0 + m * 60_000;

// ── Stufen der Ampel ──────────────────────────────────────────────────────
ok("frisch aufgerufen: 1. Aufruf, grün",
  aufrufUhr(T0 + 5_000, T0, ct, false), { uhr: "0:05", label: "1. Aufruf", stufe: "ok" });
ok("kurz vor der zweiten Schwelle noch grün",
  aufrufUhr(min(2) - 1_000, T0, ct, false), { uhr: "1:59", label: "1. Aufruf", stufe: "ok" });
ok("genau an der zweiten Schwelle: 2. Aufruf, gelb",
  aufrufUhr(min(2), T0, ct, false), { uhr: "2:00", label: "2. Aufruf", stufe: "warn" });
ok("ab der dritten Schwelle: Letzter Aufruf, rot",
  aufrufUhr(min(4) + 30_000, T0, ct, false), { uhr: "4:30", label: "Letzter Aufruf", stufe: "due" });
ok("Minuten über 9 werden nicht abgeschnitten",
  aufrufUhr(min(12) + 7_000, T0, ct, false), { uhr: "12:07", label: "Letzter Aufruf", stufe: "due" });

// ── Schwellen ohne Wert ───────────────────────────────────────────────────
// Eine Schwelle 0 (oder fehlend, alter Server) schaltet die jeweilige Stufe
// ab — die Uhr läuft dann grün weiter statt sofort rot zu werden.
ok("dritte Schwelle 0 → nie 'Letzter Aufruf'",
  aufrufUhr(min(30), T0, { enabled: true, secondCallMinutes: 2, thirdCallMinutes: 0 }, false),
  { uhr: "30:00", label: "2. Aufruf", stufe: "warn" });
ok("beide Schwellen fehlen → nur die Uhr, grün",
  aufrufUhr(min(30), T0, { enabled: true }, false),
  { uhr: "30:00", label: "1. Aufruf", stufe: "ok" });

// ── Wann die Uhr NICHT erscheint ──────────────────────────────────────────
ok("Timer in den Einstellungen aus → keine Uhr",
  aufrufUhr(min(3), T0, { enabled: false, secondCallMinutes: 2, thirdCallMinutes: 4 }, false), null);
ok("kein callTimer im Umschlag (alter Server) → keine Uhr",
  aufrufUhr(min(3), T0, undefined, false), null);
ok("kein Aufruf-Stempel → keine Uhr",
  aufrufUhr(min(3), null, ct, false), null);
ok("Stempel 0 (Neustart-Lage ohne Stempel) → keine Uhr",
  aufrufUhr(min(3), 0, ct, false), null);
// Sobald das Tablet im Spiel ist, übernimmt die Spieldauer die Kopfzeile.
ok("Tablet im Spiel → keine Uhr mehr",
  aufrufUhr(min(3), T0, ct, true), null);

// ── Uhren-Drift ───────────────────────────────────────────────────────────
// Liegt der Stempel „in der Zukunft" (Tablet-Uhr geht nach, Offset noch
// nicht gemessen), darf keine negative Zeit erscheinen.
ok("Stempel in der Zukunft → 0:00",
  aufrufUhr(T0 - 5_000, T0, ct, false), { uhr: "0:00", label: "1. Aufruf", stufe: "ok" });

// ── Das eigene Feld aus der /health-Antwort ───────────────────────────────
// LAN wie Cloud liefern `courts[]` mit `court_id`, `match_id`, `sets` und
// `on_court_since_ms`, daneben `callTimer`. Der schmale Abruf (`?court=`)
// bringt nur ein Feld — die Suche nach der ID schadet dann nicht und schützt
// vor einem älteren Server, der den Filter ignoriert und alle Felder schickt.
const antwort = {
  ok: true,
  courts: [
    { court_id: 7, court: "1", match_id: 42, sets: [[0, 0]], on_court_since_ms: T0 },
    { court_id: 8, court: "2", match_id: 0, sets: [], on_court_since_ms: null },
    { court_id: 9, court: "3", match_id: 43, sets: [[21, 15], [3, 4]], on_court_since_ms: T0 },
  ],
  serverNowMs: T0 + 1000,
  callTimer: ct,
};
ok("eigenes Feld gefunden, noch kein Punkt",
  feldAusHealth(antwort, 7), { gefunden: true, matchId: 42, onCourtSinceMs: T0, gespielt: false, callTimer: ct });
ok("Feld ohne Spiel: Stempel null",
  feldAusHealth(antwort, 8), { gefunden: true, matchId: 0, onCourtSinceMs: null, gespielt: false, callTimer: ct });
// Ein frisch angesteckter Ersatz-Tablet ohne lokalen Stand sieht am
// Satzstand des Servers, dass längst gespielt wird — die Uhr bleibt weg.
ok("LAN-Satzstand [[a,b]] mit Punkten → gespielt",
  feldAusHealth(antwort, 9).gespielt, true);
ok("Relay-Satzstand [{a,b}] mit Punkten → gespielt",
  feldAusHealth({ courts: [{ court_id: 7, match_id: 1, sets: [{ a: 0, b: 1 }], on_court_since_ms: T0 }], callTimer: ct }, 7).gespielt,
  true);
ok("Relay-Satzstand [{a,b}] nur 0:0 → nicht gespielt",
  feldAusHealth({ courts: [{ court_id: 7, match_id: 1, sets: [{ a: 0, b: 0 }], on_court_since_ms: T0 }], callTimer: ct }, 7).gespielt,
  false);
// Der Server kennt das Feld (noch) nicht — Relay direkt nach dem Neustart,
// bevor der Host seine Feldliste hochgeladen hat. Das ist kein „kein Spiel",
// sondern „weiß nicht": der Aufrufer behält seinen Stand.
ok("Feld nicht in der Antwort → nicht gefunden, Schwellen bleiben",
  feldAusHealth(antwort, 99), { gefunden: false, matchId: 0, onCourtSinceMs: null, gespielt: false, callTimer: ct });
ok("leere/kaputte Antwort schadet nicht",
  feldAusHealth(null, 7), { gefunden: false, matchId: 0, onCourtSinceMs: null, gespielt: false, callTimer: null });
ok("Antwort ohne courts schadet nicht",
  feldAusHealth({ ok: true }, 7), { gefunden: false, matchId: 0, onCourtSinceMs: null, gespielt: false, callTimer: null });

if (failures > 0) {
  console.error(`\n${failures} Fehler`);
  process.exit(1);
}
console.log("\nAlle Prüfungen bestanden.");
