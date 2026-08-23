// Testet die Retry-Regel des Ergebnisses (src/io/resultRetry.mjs) — das echte
// Modul, dessen Inline-Kopie assets/tablet.html trägt.
//
// Hintergrund: Feldtest Köpi-Cup 22.08.2026. Tablets übertrugen Punkte, konnten
// ihr Spiel aber nicht abschließen und versprachen dabei endlos „wird
// automatisch wiederholt, bis es ankommt". Ein Fehler hier kostet im besten
// Fall einen überflüssigen HTTP-Request — im schlimmsten ein Ergebnis.
import { naechsterSchritt, OHNE_GRUND } from "../src/io/resultRetry.mjs";

let failures = 0;
function ok(name, got, want) {
  if (got !== want) {
    console.error(`✗ ${name}: erwartet ${JSON.stringify(want)}, war ${JSON.stringify(got)}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

const art = (antwort) => naechsterSchritt(antwort).art;
const grund = (antwort) => naechsterSchritt(antwort).grund;

// ── Erfolg ───────────────────────────────────────────────────────────────
ok("angenommen", art({ ok: true }), "ok");
ok("angenommen trägt keinen Grund", grund({ ok: true }), "");
ok(
  "angenommen schlägt permanent",
  art({ ok: true, permanent: true }),
  "ok",
);

// ── Keine verwertbare Antwort ⇒ immer wiederholen ────────────────────────
ok("Netzfehler (null)", art(null), "wiederholen");
ok("Netzfehler (undefined)", art(undefined), "wiederholen");
ok("kaputtes JSON (String)", art("<html>502</html>"), "wiederholen");
ok("kaputtes JSON (Zahl)", art(0), "wiederholen");

// ── Wiederholbare Ablehnungen ────────────────────────────────────────────
ok(
  "kein Match auf dem Feld",
  art({ ok: false, error: "Kein Match auf diesem Court." }),
  "wiederholen",
);
ok(
  "Match gewechselt",
  art({ ok: false, error: "Das Match auf dem Court hat inzwischen gewechselt." }),
  "wiederholen",
);
ok(
  "alter Host ohne das Feld bleibt wiederholbar",
  art({ ok: false, error: "irgendwas" }),
  "wiederholen",
);
ok(
  "permanent:false ist ausdrücklich wiederholbar",
  art({ ok: false, error: "x", permanent: false }),
  "wiederholen",
);

// ── Dauerhafte Ablehnungen ───────────────────────────────────────────────
ok(
  "Satz passt nicht zur Zählweise",
  art({
    ok: false,
    permanent: true,
    error: "Satz 13:8 ist nicht regulär zu Ende gespielt (bis 21, Deckel 30).",
  }),
  "dauerhaft",
);
ok(
  "der Grund wird durchgereicht",
  grund({ ok: false, permanent: true, error: "Satz 13:8 passt nicht." }),
  "Satz 13:8 passt nicht.",
);

// ── Grund-Ersatztext ─────────────────────────────────────────────────────
ok("ohne Grund", grund({ ok: false, permanent: true }), OHNE_GRUND);
ok("leerer Grund", grund({ ok: false, permanent: true, error: "   " }), OHNE_GRUND);
ok("Grund kein String", grund({ ok: false, permanent: true, error: 42 }), OHNE_GRUND);

// ── Im Zweifel wiederholen ───────────────────────────────────────────────
ok(
  "permanent als String zählt nicht als Ja",
  art({ ok: false, error: "x", permanent: "true" }),
  "wiederholen",
);
ok(
  "permanent als 1 zählt nicht als Ja",
  art({ ok: false, error: "x", permanent: 1 }),
  "wiederholen",
);

if (failures) {
  console.error(`\n${failures} Test(s) fehlgeschlagen.`);
  process.exit(1);
}
console.log("\nAlle Tests der Ergebnis-Retry-Regel bestanden.");
