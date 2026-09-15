// Testet die Fünf-Minuten-Freigabe der Zahnrad-PIN
// (src/io/pinFreigabe.mjs) — das echte Modul, dessen Inline-Kopie
// tablet.html und anzeige.html tragen. Hier entscheidet sich, wann das
// Zahnrad ohne PIN öffnet: alles, was hier durchrutscht, umgeht die PIN.
import { FREIGABE_MS, freigabeGueltig, neueFreigabe, freigabeLesen } from "../src/io/pinFreigabe.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

const JETZT = 1_800_000_000_000;

// ── Dauer ────────────────────────────────────────────────────────────────
ok("Freigabe dauert fünf Minuten", FREIGABE_MS, 5 * 60 * 1000);
ok("neue Freigabe = jetzt + fünf Minuten", neueFreigabe(JETZT), JETZT + FREIGABE_MS);

// ── Gültigkeit ───────────────────────────────────────────────────────────
const bis = neueFreigabe(JETZT);
ok("direkt nach Eingabe gültig", freigabeGueltig(bis, JETZT), true);
ok("nach 4:59 noch gültig", freigabeGueltig(bis, JETZT + FREIGABE_MS - 1000), true);
ok("genau bei Ablauf ungültig", freigabeGueltig(bis, JETZT + FREIGABE_MS), false);
ok("nach 5:01 ungültig", freigabeGueltig(bis, JETZT + FREIGABE_MS + 1000), false);
ok("ohne Freigabe ungültig", freigabeGueltig(null, JETZT), false);
ok("undefined ungültig", freigabeGueltig(undefined, JETZT), false);
ok("NaN ungültig", freigabeGueltig(NaN, JETZT), false);
ok("0 ungültig", freigabeGueltig(0, JETZT), false);
// Uhr zurückgestellt oder manipulierter Wert weit in der Zukunft: Eine
// Freigabe, die länger als die Dauer gilt, ist keine echte — verwerfen.
ok("Freigabe weiter als fünf Minuten in der Zukunft ungültig", freigabeGueltig(JETZT + 2 * FREIGABE_MS, JETZT), false);
ok("Freigabe genau fünf Minuten in der Zukunft gültig", freigabeGueltig(JETZT + FREIGABE_MS, JETZT), true);

// ── Lesen aus dem Speicher (String aus localStorage) ─────────────────────
ok("Zahl-String lesen", freigabeLesen(String(bis)), bis);
ok("null lesen → null", freigabeLesen(null), null);
ok("leer lesen → null", freigabeLesen(""), null);
ok("Unsinn lesen → null", freigabeLesen("abc"), null);
ok("1e3 lesen → null", freigabeLesen("1e3"), null);
ok("negativ lesen → null", freigabeLesen("-5"), null);
ok("Dezimal lesen → null", freigabeLesen("12.5"), null);

if (failures) { console.error(`${failures} Test(s) fehlgeschlagen`); process.exit(1); }
console.log("alle Tests grün");
