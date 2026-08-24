// Testet die Server-Uhr (src/io/monitorClock.mjs, Etappe ETag 1c) — das echte
// Modul, dessen Inline-Kopie monitor.html trägt.
import { clockFeed, nowServerMs, clockHatOffset } from "../src/io/monitorClock.mjs";

let failures = 0;
function ok(name, got, want) {
  if (got !== want) {
    console.error(`✗ ${name}: erwartet ${want}, war ${got}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

const LOKAL = 1_787_000_000_000; // lokale Uhr (Pi, evtl. drifttend)
const SERVER = 1_787_000_360_000; // Server-Uhr ist 6 min voraus

// ── Kaltstart ───────────────────────────────────────────────────────────────
ok("ohne Offset fällt auf die lokale Uhr zurück", nowServerMs(LOKAL), LOKAL);
ok("ohne Offset ist kein Offset gesetzt", clockHatOffset(), false);

// ── Fütterung aus einem 200-Abruf (state.serverNowMs) ───────────────────────
ok("200-Body setzt den Offset", clockFeed(SERVER, LOKAL), true);
ok("danach ist ein Offset gesetzt", clockHatOffset(), true);
ok("nowServerMs rechnet relativ zur Server-Uhr", nowServerMs(LOKAL), SERVER);
// Eine Sekunde später: beide Uhren ticken — der Abstand bleibt erhalten.
ok(
  "jetztMs eine Sekunde später: Server-Zeit folgt",
  nowServerMs(LOKAL + 1000),
  SERVER + 1000,
);

// ── Ungültige Fütterung wird ignoriert ───────────────────────────────────────
ok("0 wird ignoriert", clockFeed(0, LOKAL), false);
ok("negativ wird ignoriert", clockFeed(-5, LOKAL), false);
ok("Offset bleibt nach Müll-Fütterung bestehen", nowServerMs(LOKAL), SERVER);

// ── 304-Serie: der Offset lebt weiter (kein Reset, keine Ableitung aus 304) ─
ok("Offset überlebt eine 304-Serie", nowServerMs(LOKAL + 5000), SERVER + 5000);

// ── WS-Herzschlag ({"hb":<nowMs>}) frischt den Offset ───────────────────────
ok("Herzschlag setzt den Offset", clockFeed(SERVER + 9000, LOKAL + 9000), true);
ok(
  "jetztMs eine Sekunde später: Herzschlag-Offset wirkt",
  nowServerMs(LOKAL + 10000),
  SERVER + 10000,
);

// ── Monoton plausibel ────────────────────────────────────────────────────────
// Für aufeinanderfolgende, wachsende `jetztMs` muss `nowServerMs` wachsen —
// sonst spränge ein Countdown rückwärts (U12).
let letzter = -1;
let monoton = true;
for (let j = LOKAL + 1000; j <= LOKAL + 20000; j += 250) {
  const w = nowServerMs(j);
  if (w < letzter) { monoton = false; break; }
  letzter = w;
}
ok("nowServerMs bleibt monoton plausibel", monoton, true);

if (failures > 0) {
  console.error(`\n${failures} Fehler`);
  process.exit(1);
}
console.log("\nAlle Prüfungen bestanden.");
