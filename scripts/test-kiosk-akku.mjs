// Wächter gegen Drift der Akku-Schwellen (Spec kiosk-akku-und-ausgang): Die
// Wartekarte der Kiosk-App (android/…/kern/AkkuAnzeige.kt) färbt den Akku
// mit denselben Schwellen wie die Tablet-Seiten (src/io/akku.mjs). Weichen
// sie ab, sähe derselbe Akku am selben Gerät je nach Ansicht anders aus.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { AKKU_NIEDRIG, AKKU_KRITISCH, akkuText } from "../src/io/akku.mjs";

const wurzel = join(dirname(fileURLToPath(import.meta.url)), "..");
const kt = readFileSync(
  join(wurzel, "android/app/src/main/kotlin/de/badhub/btslight/tablet/kern/AkkuAnzeige.kt"),
  "utf8",
);
const konst = (name) => {
  const m = kt.match(new RegExp(`const val ${name} = (\\d+)`));
  if (!m) throw new Error(`AkkuAnzeige.kt: ${name} nicht gefunden`);
  return Number(m[1]);
};

let fehler = 0;
function ok(name, got, want) {
  if (got !== want) { console.error(`✗ ${name}: erwartet ${want}, war ${got}`); fehler++; }
  else console.log(`✓ ${name}`);
}
ok("Schwelle niedrig", konst("NIEDRIG"), AKKU_NIEDRIG);
ok("Schwelle kritisch", konst("KRITISCH"), AKKU_KRITISCH);
// Die Zeichen stehen als Literale im Kotlin-Code — gleiche Symbole wie die Seiten.
ok("Symbol Akku", kt.includes('"🔋 "'), akkuText(50, false).startsWith("🔋 "));
ok("Symbol Laden", kt.includes('"⚡ "'), akkuText(50, true).startsWith("⚡ "));

if (fehler) process.exit(1);
console.log("\nalle Tests grün");
