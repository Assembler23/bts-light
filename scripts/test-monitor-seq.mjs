// Testet die Ordnung zwischen Push und Voll-Abruf (src/io/monitorSeq.mjs,
// Spec monitor-livestand-push S4) — das echte Modul, dessen Inline-Kopie die
// Anzeige-Seiten tragen.
import { anwenden } from "../src/io/monitorSeq.mjs";

let failures = 0;
function ok(name, got, want) {
  if (got !== want) {
    console.error(`✗ ${name}: erwartet ${want}, war ${got}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

// ── Push: muss echt neuer sein ────────────────────────────────────────────
ok("Push mit größerem seq gilt", anwenden(100, 101, "push"), true);
ok("Push mit gleichem seq gilt NICHT", anwenden(100, 100, "push"), false);
ok("Push mit kleinerem seq gilt NICHT", anwenden(100, 99, "push"), false);

// ── Voll-Abruf: darf gleichziehen ─────────────────────────────────────────
// Das Gleichheitszeichen ist der BTP-Rückfall-Fall: Nimmt jemand dort einen
// Satzstand von Hand zurück, ändert sich der Inhalt, ohne dass ein Nudge
// dazwischenliegt — die Antwort muss trotzdem angewendet werden.
ok("Abruf mit gleichem seq gilt", anwenden(100, 100, "fetch"), true);
ok("Abruf mit größerem seq gilt", anwenden(100, 101, "fetch"), true);
ok("Abruf mit kleinerem seq gilt NICHT", anwenden(100, 99, "fetch"), false);

// ── Ohne Ordnung nie blockieren ───────────────────────────────────────────
// Ein älterer Server schickt das Feld nicht (0). Dann verhält sich die Seite
// wie vor der Etappe — eine eingefrorene Anzeige wäre der schlimmere Fehler.
ok("seq 0 (alter Server) gilt immer", anwenden(100, 0, "push"), true);
ok("noch nichts gezeigt: gilt immer", anwenden(0, 5, "push"), true);
ok("beides 0 gilt", anwenden(0, 0, "fetch"), true);

// ── Unfug schadet nicht ───────────────────────────────────────────────────
ok("unbekannte Quelle wird wie ein Abruf behandelt", anwenden(100, 100, "irgendwas"), true);
ok("NaN im gezeigten Wert blockiert nicht", anwenden(NaN, 100, "push"), true);
ok("NaN im neuen Wert blockiert nicht", anwenden(100, NaN, "push"), true);
ok("undefined blockiert nicht", anwenden(undefined, undefined, "push"), true);

// ── Der Neustart-Fall ─────────────────────────────────────────────────────
// Weil die Sequenz bei der Uhrzeit startet, ist sie nach einem Neustart des
// Turnier-PCs GRÖSSER als die gemerkte — die Anzeige darf nicht hängen.
ok(
  "nach einem Neustart ist die neue Sequenz größer",
  anwenden(1787000000005, 1787900000001, "push"),
  true,
);

if (failures > 0) {
  console.error(`\n${failures} Fehler`);
  process.exit(1);
}
console.log("\nAlle Prüfungen bestanden.");
