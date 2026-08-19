// Testet die Gesundheits-Regel des Push-Kanals (src/io/pushHealth.mjs, Spec
// monitor-livestand-push S6) — das echte Modul, dessen Inline-Kopie die
// Anzeige-Seiten tragen.
//
// Laut Spec das höchste Risiko im ganzen Vorhaben: Eine falsche Regel hier
// friert alle Anzeigen einer Halle ein.
import {
  pushGesund,
  fallbackTakt,
  kanalIstTot,
  HERZSCHLAG_STILL_MS,
  FALLBACK_LANGSAM_MS,
  FALLBACK_SCHNELL_MS,
} from "../src/io/pushHealth.mjs";

let failures = 0;
function ok(name, got, want) {
  if (got !== want) {
    console.error(`✗ ${name}: erwartet ${want}, war ${got}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

const JETZT = 1_787_000_000_000;
/** Gesunder Ausgangszustand: Socket offen, Frame gerade eben, kein Fehler. */
const gut = (extra) =>
  Object.assign(
    { wsOpen: true, lastServerFrameMs: JETZT - 1000, lastFetchOk: true, failures: 0 },
    extra || {},
  );

// ── Der Normalfall ────────────────────────────────────────────────────────
ok("frischer Anstoß = gesund", pushGesund(gut(), JETZT), true);

// ── Die ruhige Halle: der eigentliche Grund für den Herzschlag ────────────
// Zwischen zwei Ballwechseln passiert minutenlang nichts. Ohne Herzschlag
// wäre das von einem toten Kanal nicht zu unterscheiden — die Anzeige fiele
// grundlos auf den schnellen Takt zurück oder, schlimmer, verschliefe einen
// echten Ausfall.
ok(
  "ruhige Halle mit Herzschlag vor 11 s = gesund",
  pushGesund(gut({ lastServerFrameMs: JETZT - 11000 }), JETZT),
  true,
);
ok(
  "Herzschlag vor 24 s = noch gesund",
  pushGesund(gut({ lastServerFrameMs: JETZT - 24000 }), JETZT),
  true,
);
ok(
  "Herzschlag vor 25 s = nicht mehr gesund",
  pushGesund(gut({ lastServerFrameMs: JETZT - HERZSCHLAG_STILL_MS }), JETZT),
  false,
);

// ── Halbtoter Socket ──────────────────────────────────────────────────────
// Er meldet minutenlang OPEN, ohne dass je etwas ankommt. Genau dieser Fall
// war mit der alten Regel unsichtbar.
ok(
  "offen, aber seit 60 s still = nicht gesund",
  pushGesund(gut({ lastServerFrameMs: JETZT - 60000 }), JETZT),
  false,
);
ok("Socket zu = nicht gesund", pushGesund(gut({ wsOpen: false }), JETZT), false);

// ── Ein einziger Fehlversuch genügt ───────────────────────────────────────
ok("ein Fehlversuch = nicht gesund", pushGesund(gut({ failures: 1 }), JETZT), false);
ok("fehlgeschlagener Abruf = nicht gesund", pushGesund(gut({ lastFetchOk: false }), JETZT), false);

// ── Kaltstart ─────────────────────────────────────────────────────────────
// Bis der Kanal sich bewährt hat, gilt der schnelle Takt.
ok("noch nie ein Frame = nicht gesund", pushGesund(gut({ lastServerFrameMs: 0 }), JETZT), false);
ok("leerer Zustand = nicht gesund", pushGesund(null, JETZT), false);
ok("Kaltstart pollt schnell", fallbackTakt(false, true), FALLBACK_SCHNELL_MS);

// ── Der Schalter ──────────────────────────────────────────────────────────
// Ohne ihn bleibt alles wie vorher — das ist die Zusage an bestehende
// Installationen.
ok("gesund + Schalter an = 4 s", fallbackTakt(true, true), FALLBACK_LANGSAM_MS);
ok("gesund + Schalter aus = 250 ms", fallbackTakt(true, false), FALLBACK_SCHNELL_MS);
ok("ungesund + Schalter an = 250 ms", fallbackTakt(false, true), FALLBACK_SCHNELL_MS);
ok("ungesund + Schalter aus = 250 ms", fallbackTakt(false, false), FALLBACK_SCHNELL_MS);

// Der Takt ist IMMER eine Zahl > 0 — es gibt kein „gar nicht abrufen".
// Genau das war der Blocker im ersten Wurf der Etappe: Die Anzeige-Seiten
// lasen „gesund ohne Schalter" als „Abruf ganz einstellen". Am Monitor hängt
// an diesem Abruf aber auch das Lebenszeichen des Geräts, seine Fernbefehle
// und die Feld-Zuweisung — nichts davon wird angestoßen.
for (const g of [true, false]) {
  for (const s of [true, false]) {
    const takt = fallbackTakt(g, s);
    ok(
      `Takt bleibt endlich und > 0 (gesund=${g}, Schalter=${s})`,
      Number.isFinite(takt) && takt > 0,
      true,
    );
  }
}

// ── Aktiver Reconnect ─────────────────────────────────────────────────────
ok("still seit 25 s = Kanal tot", kanalIstTot(true, JETZT - HERZSCHLAG_STILL_MS, JETZT), true);
ok("still seit 24 s = noch nicht", kanalIstTot(true, JETZT - 24000, JETZT), false);
ok("geschlossener Socket: kein Force-Close", kanalIstTot(false, JETZT - 60000, JETZT), false);
ok("noch nie ein Frame: erst abwarten", kanalIstTot(true, 0, JETZT), false);
// „Erst abwarten" darf nicht „ewig abwarten" heißen: Bricht die Leitung
// direkt nach dem Handschlag weg (WLAN-Roam), kommt nie ein Frame und die
// Seite hinge dauerhaft im schnellen Takt. Die Anzeige-Seiten reichen
// deshalb den Zeitpunkt des Verbindens als Ersatz-Bezug herein.
ok(
  "verbunden seit 25 s ohne ein einziges Frame = tot",
  kanalIstTot(true, JETZT - HERZSCHLAG_STILL_MS, JETZT),
  true,
);

// ── Zusammenspiel der beiden Grenzen ──────────────────────────────────────
// „Nicht mehr gesund" und „tot" fallen bewusst zusammen: Sobald die Anzeige
// auf den schnellen Takt zurückfällt, soll sie auch neu verbinden.
ok(
  "dieselbe Grenze für ungesund und tot",
  pushGesund(gut({ lastServerFrameMs: JETZT - HERZSCHLAG_STILL_MS }), JETZT) === false &&
    kanalIstTot(true, JETZT - HERZSCHLAG_STILL_MS, JETZT) === true,
  true,
);

if (failures > 0) {
  console.error(`\n${failures} Fehler`);
  process.exit(1);
}
console.log("\nAlle Prüfungen bestanden.");
