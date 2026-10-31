// Testet die Regel der Beendet-Ansicht am Zähl-Tablet
// (src/io/abschlussAnsicht.mjs) — das echte Modul, dessen Inline-Kopie
// assets/tablet.html trägt.
//
// Hintergrund: Turnier 12./13.09.2026, Feld 11. Das Ergebnis war gesendet und
// in BTP fest, das Tablet zurückgetreten. Trotzdem ließ sich „Korrektur —
// Match wieder öffnen" drücken, und danach schluckte das Finalisiert-Gate
// 23-mal hintereinander den Knopf „Ergebnis übermitteln" — ohne ein Wort auf
// dem Schirm. Was hier geprüft wird: Ein festes Ergebnis sperrt BEIDE Knöpfe
// und sagt, wohin man sich wenden muss; alle anderen Lagen bleiben, wie sie
// waren.
import { abschlussLage, sendeauftragErledigt } from "../src/io/abschlussAnsicht.mjs";

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

const lage = (z) => abschlussLage(Object.assign({
  finalized: false, submittedOk: false, resultRejected: null,
  pendingResult: false, needWinner: false,
}, z));

// ── Fest in BTP: beide Knöpfe zu, klare Ansage ───────────────────────────
ok(
  "finalisiert nach eigenem Senden",
  lage({ finalized: true, submittedOk: true }),
  { sendenGesperrt: true, korrekturGesperrt: true,
    status: "✓ Ergebnis steht in BTP fest — Korrektur nur über die Turnierleitung." },
);
ok(
  "finalisiert von Hand (Tablet hatte nie gesendet)",
  lage({ finalized: true }),
  { sendenGesperrt: true, korrekturGesperrt: true,
    status: "✓ Ergebnis steht in BTP fest — Korrektur nur über die Turnierleitung." },
);
ok(
  "finalisiert schlägt eine ältere Ablehnung",
  lage({ finalized: true, resultRejected: "Satzliste unstimmig" }).korrekturGesperrt,
  true,
);
ok(
  "finalisiert schlägt einen laufenden Versuch",
  lage({ finalized: true, pendingResult: true }).status,
  "✓ Ergebnis steht in BTP fest — Korrektur nur über die Turnierleitung.",
);

// ── Alles andere wie bisher: Korrektur bleibt möglich ────────────────────
ok(
  "übermittelt, noch nicht fest",
  lage({ submittedOk: true }),
  { sendenGesperrt: true, korrekturGesperrt: false,
    status: "✓ Übermittelt — die Turnierleitung kann übernehmen." },
);
ok(
  "dauerhaft abgelehnt: Knopf frei, Grund steht da",
  lage({ resultRejected: "Satzliste unstimmig" }),
  { sendenGesperrt: false, korrekturGesperrt: false,
    status: "✗ Nicht angenommen: Satzliste unstimmig — bitte den Stand prüfen oder die Turnierleitung ansprechen." },
);
ok(
  "wird übermittelt",
  lage({ pendingResult: true }),
  { sendenGesperrt: true, korrekturGesperrt: false,
    status: "Ergebnis wird übermittelt … wird automatisch wiederholt, bis es ankommt." },
);
ok(
  "Sieger fehlt noch (Aufgabe/Kampflos)",
  lage({ needWinner: true }),
  { sendenGesperrt: true, korrekturGesperrt: false, status: "Bitte zuerst den Sieger wählen." },
);
ok(
  "bereit zum Senden",
  lage({}),
  { sendenGesperrt: false, korrekturGesperrt: false, status: "" },
);

// ── Unsinnige Eingaben ───────────────────────────────────────────────────
ok("kein Zustand = bereit", abschlussLage(null), { sendenGesperrt: false, korrekturGesperrt: false, status: "" });

// ── Offener Sendeauftrag, wenn BTP das Match festmacht ───────────────────
//
// Review-Fund 15.09.2026: Trifft das Finalisiert-Frame ein, während `/result`
// noch unterwegs oder im Retry ist, kehrte der Retry am Gate zurück, ohne den
// Auftrag loszulassen — und beim NÄCHSTEN Match blockte er still den
// Sende-Knopf („wird übermittelt … bis es ankommt", für ein Spiel, das längst
// in BTP steht).
const auftrag = { matchId: 1418, sets: [] };
ok("fest in BTP für dasselbe Match: Auftrag ist erledigt", sendeauftragErledigt(auftrag, 1418, true), true);
ok("nicht fest: Auftrag bleibt", sendeauftragErledigt(auftrag, 1418, false), false);
ok("fest, aber anderes Match: Auftrag bleibt", sendeauftragErledigt(auftrag, 1419, true), false);
ok("kein Auftrag: nichts zu erledigen", sendeauftragErledigt(null, 1418, true), false);
ok("Match unbekannt: Auftrag bleibt", sendeauftragErledigt(auftrag, null, true), false);

if (failures) {
  console.error(`\n${failures} Test(s) fehlgeschlagen.`);
  process.exit(1);
}
console.log("\nAlle Tests der Beendet-Ansicht bestanden.");
