// Schriftgröße der Ansage im Schiri-Modus (src/io/ansageSchrift.mjs): Die
// Ansage nimmt die größte Stufe, die passt — kurze Texte groß, lange nie
// kleiner als früher, außer sie liefen sonst aus dem Bild.
import { ansageStufen, ansageKandidaten, ansageSchriftWaehlen, grosseSchriftAus } from "../src/io/ansageSchrift.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

ok("Kandidaten normal: 2,6 … 1,0", ansageKandidaten(false), [2.6, 2.4, 2.2, 2, 1.8, 1.6, 1.4, 1.2, 1]);
ok("Kandidaten groß: 3,6 … 1,2", [ansageKandidaten(true)[0], ansageKandidaten(true).at(-1)], [3.6, 1.2]);
ok("Basis normal = bisherige Größe", ansageStufen(false).basis, 1.6);
ok("groß ist in jeder Stufe größer", ["max", "basis", "not"].every((k) => ansageStufen(true)[k] > ansageStufen(false)[k]), true);

// `messe` nachgebildet: Überlauf ab einer Größe, Feld-Anteil sinkt mit ihr.
const messe = (ueberAb, feldBei) => (rem) => ({ ueberlauf: rem >= ueberAb, feldAnteil: feldBei ? feldBei(rem) : null });

ok("kurze Ansage, alles passt → größte Stufe", ansageSchriftWaehlen(false, messe(99)), 2.6);
ok("groß: kurze Ansage → 3,6", ansageSchriftWaehlen(true, messe(99)), 3.6);
ok("Überlauf ab 2,2 → 2,0", ansageSchriftWaehlen(false, messe(2.2)), 2);
ok("Feld bekäme zu wenig → kleiner, bis es reicht",
  ansageSchriftWaehlen(false, messe(99, (rem) => (rem > 2 ? 0.2 : 0.4))), 2);
ok("Feld neben der Leiste (null) bremst nicht", ansageSchriftWaehlen(false, messe(99, () => null)), 2.6);
ok("lange Ansage: Feld reicht nie → Basis, wie früher",
  ansageSchriftWaehlen(false, messe(99, () => 0.05)), 1.6);
ok("groß: Feld reicht nie → Basis 2,0", ansageSchriftWaehlen(true, messe(99, () => 0.05)), 2);
ok("Überlauf selbst bei Basis → darunter", ansageSchriftWaehlen(false, messe(1.4)), 1.2);
ok("unter der Basis zählt das Feld nicht mehr", ansageSchriftWaehlen(false, messe(1.6, () => 0)), 1.4);
ok("nichts passt → kleinste Stufe", ansageSchriftWaehlen(false, messe(0)), 1);
ok("groß: nichts passt → 1,2", ansageSchriftWaehlen(true, messe(0)), 1.2);
ok("messe ohne Ergebnis gilt als passend", ansageSchriftWaehlen(false, () => undefined), 2.6);

ok("Schalter: '1' an", grosseSchriftAus("1"), true);
ok("Schalter: null aus", grosseSchriftAus(null), false);
ok("Schalter: '0' aus", grosseSchriftAus("0"), false);

if (failures) { console.error(`\n${failures} Fehler`); process.exit(1); }
console.log("\nAlle Tests grün.");
