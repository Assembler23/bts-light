// Testet den Stillstands-Wächter der Anzeige-Seiten (src/io/standstill.mjs) —
// das echte Modul, dessen Inline-Kopie overview.html und monitor.html tragen.
//
// Hintergrund: Court-Monitore, deren Stand einfror, während die Seite
// weiterlief (Feldtest 22.08.2026). Das Symptom erzeugte bis dahin KEINE
// Log-Spur: Der Upload hing an JS-Fehler, unhandledrejection und pagehide —
// ein stiller Hänger fällt durch alle drei Raster.
//
// Ein Fehlalarm hier ist teuer: Er schickt Log-Uploads von zwanzig Geräten
// los und lässt eine gesunde Halle krank aussehen. Deshalb prüfen die Tests
// vor allem die Fälle, in denen NICHTS gemeldet werden darf.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  lagePruefen,
  STILLSTAND_MS,
} from "../src/io/standstill.mjs";

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

const JETZT = 1_787_000_000_000;
const vorMs = (ms) => JETZT - ms;

/** Gesunde Ausgangslage: gerade eben abgerufen, gerade eben angewendet. */
const gesund = (extra) =>
  Object.assign(
    {
      startMs: vorMs(10 * 60_000),
      letzterAbrufOkMs: vorMs(1_000),
      letzterStandMs: vorMs(1_000),
      gemeldeteArt: null,
    },
    extra,
  );

const art = (z) => lagePruefen(z, JETZT).art;
const melden = (z) => lagePruefen(z, JETZT).melden;

// ── Gesund: nichts melden ────────────────────────────────────────────────
ok("gesunde Anzeige", art(gesund()), null);
ok("gesunde Anzeige meldet nicht", melden(gesund()), false);
ok(
  "knapp unter der Schwelle ist noch gesund",
  art(gesund({ letzterStandMs: vorMs(STILLSTAND_MS - 1) })),
  null,
);
ok(
  "eine ruhige Halle ist kein Stillstand",
  // Nichts passiert auf dem Feld — aber der Sicherheits-Poll bestätigt den
  // Stand weiter (304 zählt wie ein angewendeter Stand).
  art(gesund({ letzterStandMs: vorMs(3_000), letzterAbrufOkMs: vorMs(3_000) })),
  null,
);

// ── Antworten kommen an, werden aber verworfen ───────────────────────────
ok(
  "Abrufe klappen, nichts wird übernommen",
  art(gesund({ letzterStandMs: vorMs(STILLSTAND_MS + 1) })),
  "verworfen",
);
ok(
  "und das wird gemeldet",
  melden(gesund({ letzterStandMs: vorMs(STILLSTAND_MS + 1) })),
  true,
);
ok(
  "die Stillstandsdauer steht im Bericht",
  lagePruefen(gesund({ letzterStandMs: vorMs(90_000) }), JETZT).stillMs,
  90_000,
);

// ── Gar keine geglückten Abrufe mehr ─────────────────────────────────────
ok(
  "der Abruf selbst ist tot",
  art(gesund({ letzterAbrufOkMs: vorMs(STILLSTAND_MS + 1), letzterStandMs: vorMs(STILLSTAND_MS + 1) })),
  "keine_abrufe",
);
ok(
  "toter Abruf schlägt „verworfen“",
  // Beides ist alt — die schwerwiegendere und genauere Aussage gewinnt.
  art(gesund({ letzterAbrufOkMs: vorMs(120_000), letzterStandMs: vorMs(200_000) })),
  "keine_abrufe",
);

// ── Startphase ───────────────────────────────────────────────────────────
ok(
  "frisch geladen, noch nichts empfangen",
  art({ startMs: vorMs(2_000), letzterAbrufOkMs: 0, letzterStandMs: 0, gemeldeteArt: null }),
  null,
);
ok(
  "nach dem Start nie etwas empfangen ist meldenswert",
  art({ startMs: vorMs(STILLSTAND_MS + 1), letzterAbrufOkMs: 0, letzterStandMs: 0, gemeldeteArt: null }),
  "keine_abrufe",
);

// ── Nur einmal je Episode melden ─────────────────────────────────────────
ok(
  "dieselbe Lage wird nicht erneut gemeldet",
  melden(gesund({ letzterStandMs: vorMs(200_000), gemeldeteArt: "verworfen" })),
  false,
);
ok(
  "eine ANDERE Lage wird gemeldet",
  melden(
    gesund({
      letzterAbrufOkMs: vorMs(200_000),
      letzterStandMs: vorMs(200_000),
      gemeldeteArt: "verworfen",
    }),
  ),
  true,
);

// ── Erholung ─────────────────────────────────────────────────────────────
ok(
  "nach der Erholung ist die Lage wieder null",
  art(gesund({ gemeldeteArt: "verworfen" })),
  null,
);
ok(
  "die Erholung wird einmal vermerkt",
  lagePruefen(gesund({ gemeldeteArt: "verworfen" }), JETZT).erholt,
  true,
);
ok(
  "ohne vorherige Meldung gibt es keine Erholung",
  lagePruefen(gesund(), JETZT).erholt,
  false,
);

// ── Unsinnige Eingaben dürfen nichts auslösen ────────────────────────────
ok("kein Zustand", art(null), null);
ok("leerer Zustand", art({}), null);
ok(
  "Zeitstempel aus der Zukunft (Uhrsprung) meldet nicht",
  art(gesund({ letzterStandMs: JETZT + 5_000 })),
  null,
);

// ── Die Aufrufer: zählt ein 304 in BEIDEN Anzeige-Seiten als Lebenszeichen? ──
//
// Die Regel oben ist nur so gut wie ihre Fütterung. `monitor.html` setzte im
// 304-Zweig („nichts Neues") weder `letzterAbrufOkMs` noch `letzterStandMs`;
// bei gesundem Push-Kanal kommt ein voller 200 nur alle 10 min (Refetch-Cap),
// und 60 s danach meldete jeder Court-Monitor `keine_abrufe` — obwohl der
// Server im Sekundentakt bestätigte (Turnier 12./13.09.2026: ~50 Fehlalarme
// je Gerät und Tag, jeder mit Log-Upload). Der Modultest konnte das nicht
// sehen. Deshalb hier ein Blick in den Quelltext der Inline-Kopien: Der
// 304-Zweig muss BEIDE Stempel setzen — den Abruf über `abrufGeglueckt()`
// vor der Verzweigung, den Stand im Zweig selbst.
const hier = dirname(fileURLToPath(import.meta.url));
const seiten = [
  // Seite → Muster, an dem der 304-Zweig beginnt.
  ["monitor.html", "if (state === null) {"],
  ["overview.html", "if (paket.status === 304) {"],
];
for (const [datei, beginn] of seiten) {
  const quelle = readFileSync(join(hier, "..", "src-tauri", "assets", datei), "utf8");
  const start = quelle.indexOf(beginn);
  ok(`${datei}: 304-Zweig gefunden`, start >= 0, true);
  if (start < 0) continue;
  const ende = quelle.indexOf("return;", start);
  const zweig = quelle.slice(start, ende);
  // Davor: die letzten Zeilen vor dem Zweig, in denen der Abruf verbucht wird.
  const davor = quelle.slice(Math.max(0, start - 400), start);
  ok(`${datei}: 304 zählt als geglückter Abruf`, davor.includes("abrufGeglueckt();"), true);
  ok(`${datei}: 304 zählt als bestätigter Stand`, zweig.includes("letzterStandMs = Date.now();"), true);
  ok(`${datei}: 304 wird gezählt`, zweig.includes("bestaetigungen++;"), true);
  // Und `abrufGeglueckt` muss den Abruf auch wirklich stempeln — sonst wäre
  // der Aufruf davor nur Dekoration (Review-Fund 15.09.2026).
  const fnStart = quelle.indexOf("function abrufGeglueckt() {");
  ok(`${datei}: abrufGeglueckt gefunden`, fnStart >= 0, true);
  if (fnStart < 0) continue;
  const fnEnde = quelle.indexOf("\n  }", fnStart);
  const rumpf = quelle.slice(fnStart, fnEnde);
  ok(`${datei}: abrufGeglueckt stempelt den Abruf`, rumpf.includes("letzterAbrufOkMs = Date.now();"), true);
}

if (failures) {
  console.error(`\n${failures} Test(s) fehlgeschlagen.`);
  process.exit(1);
}
console.log("\nAlle Tests des Stillstands-Wächters bestanden.");
