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

if (failures) {
  console.error(`\n${failures} Test(s) fehlgeschlagen.`);
  process.exit(1);
}
console.log("\nAlle Tests des Stillstands-Wächters bestanden.");
