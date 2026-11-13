// Testet die Zähl-Logik des Ballzählers (src/io/ballzaehler.mjs) — das echte
// Modul, dessen Inline-Kopie tablet.html trägt (Spec tablet-ballzaehler).
import {
  BALL_MAX, ballzahlAus, ballMehr, ballWeniger,
  zaehlerSichtbar, zaehlerGesperrt, ballzahlFuerErgebnis,
} from "../src/io/ballzaehler.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

// ── Bereich ──────────────────────────────────────────────────────────────
ok("Deckel 99 wie relay_proto::MAX_SHUTTLES", BALL_MAX, 99);
ok("Start bei 0: + zählt auch den ersten Ball", ballMehr(0), 1);
ok("+ erhöht um 1", ballMehr(4), 5);
ok("+ bei 99 tut nichts", ballMehr(99), 99);
ok("− senkt um 1", ballWeniger(4), 3);
ok("− bei 0 tut nichts", ballWeniger(0), 0);
ok("98 → 99", ballMehr(98), 99);

// ── Fremde Werte (gespeicherter oder übernommener Stand) ─────────────────
ok("fehlt → 0", ballzahlAus(undefined), 0);
ok("null → 0", ballzahlAus(null), 0);
ok("Text → 0 (auch '5')", ballzahlAus("5"), 0);
ok("NaN → 0", ballzahlAus(NaN), 0);
ok("Infinity → 0", ballzahlAus(Infinity), 0);
ok("negativ → 0", ballzahlAus(-3), 0);
ok("zu groß → 99", ballzahlAus(250), 99);
ok("Bruch wird abgeschnitten", ballzahlAus(3.9), 3);
ok("gültig bleibt", ballzahlAus(7), 7);
ok("+ auf kaputtem Wert beginnt bei 0", ballMehr("x"), 1);
ok("− auf kaputtem Wert bleibt 0", ballWeniger(undefined), 0);

// ── Sichtbarkeit: nur wenn der Turnier-PC den Zähler anbietet ────────────
ok("Host bietet an + Spiel → sichtbar", zaehlerSichtbar({ hostZaehlt: true, hatSpiel: true }), true);
ok("alter Host (Feld fehlt) → kein Zähler", zaehlerSichtbar({ hatSpiel: true }), false);
ok("kein Spiel → kein Zähler", zaehlerSichtbar({ hostZaehlt: true, hatSpiel: false }), false);
ok("ohne Lage → kein Zähler", zaehlerSichtbar(null), false);

// ── Einfrieren ab dem Übermitteln ────────────────────────────────────────
ok("im Spiel frei", zaehlerGesperrt({}), false);
ok("Auftrag unterwegs → gesperrt", zaehlerGesperrt({ unterwegs: true }), true);
ok("angenommen → gesperrt", zaehlerGesperrt({ angenommen: true }), true);
ok("von Hand gewertet → gesperrt", zaehlerGesperrt({ finalisiert: true }), true);
ok("wieder geöffnet → frei", zaehlerGesperrt({ unterwegs: false, angenommen: false }), false);
ok("ohne Lage frei", zaehlerGesperrt(undefined), false);

// ── Ergebnis-Body ────────────────────────────────────────────────────────
ok("Zähler an → Zahl im Body", ballzahlFuerErgebnis(true, 6), 6);
ok("Zähler an, nichts gezählt → 0 im Body (eine Angabe)", ballzahlFuerErgebnis(true, 0), 0);
ok("Zähler aus → Feld fehlt", ballzahlFuerErgebnis(false, 6), undefined);
ok("Zähler an, kaputter Wert → 0", ballzahlFuerErgebnis(true, "viele"), 0);

if (failures) { console.error(`\n${failures} Fehler`); process.exit(1); }
console.log("\nOK");
