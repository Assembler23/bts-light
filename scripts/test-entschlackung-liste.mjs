// Wächter gegen Drift der Entschlackungs-Liste: Die Amazon-Pakete, die die
// Tablet-Einrichtung stilllegt, stehen dreimal im Repo — als Quelle in
// android/…/kern/Entschlackung.kt und als Kopien in setup-tablet.ps1 und
// setup-tablet.sh (die Skripte laufen ohne Build, können also nicht
// importieren). Weicht eine Kopie ab, schlägt dieser Test fehl.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const wurzel = join(dirname(fileURLToPath(import.meta.url)), "..");
const lies = (p) => readFileSync(join(wurzel, p), "utf8");

// Kotlin: alle "…"-Literale innerhalb der beiden Listen, in Reihenfolge.
function ausKotlin(src) {
  const listen = [];
  for (const name of ["ALEXA", "INHALTE", "HINTERGRUND", "UPDATES"]) {
    const m = src.match(new RegExp(`val ${name}[^=]*=\\s*listOf\\(([\\s\\S]*?)\\n\\s*\\)`));
    if (!m) throw new Error(`Kotlin: Liste ${name} nicht gefunden`);
    listen.push(...[...m[1].matchAll(/"([^"]+)"/g)].map((x) => x[1]));
  }
  return listen;
}
// PowerShell: "…"-Literale im Block `$AmazonApps = @( … )`.
function ausPs1(src) {
  const m = src.match(/\$AmazonApps = @\(([\s\S]*?)\n\)/);
  if (!m) throw new Error("ps1: $AmazonApps nicht gefunden");
  return [...m[1].matchAll(/^\s*"([^"]+)"/gm)].map((x) => x[1]);
}
// Bash: Zeilen im Block `AMAZON_APPS=" … "`.
function ausSh(src) {
  const m = src.match(/AMAZON_APPS="\n([\s\S]*?)\n"/);
  if (!m) throw new Error("sh: AMAZON_APPS nicht gefunden");
  return m[1].split("\n").map((z) => z.trim()).filter(Boolean);
}

const kotlin = ausKotlin(lies("android/app/src/main/kotlin/de/badhub/btslight/tablet/kern/Entschlackung.kt"));
const ps1 = ausPs1(lies("android/setup-tablet.ps1"));
const sh = ausSh(lies("android/setup-tablet.sh"));

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}:\n  erwartet ${w}\n  war      ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}
ok("Kotlin-Liste hat Einträge", kotlin.length >= 10, true);
ok("setup-tablet.ps1 = Entschlackung.kt", ps1, kotlin);
ok("setup-tablet.sh = Entschlackung.kt", sh, kotlin);
ok("keine Doppelten", new Set(kotlin).size, kotlin.length);

if (failures) { console.error(`${failures} Test(s) fehlgeschlagen`); process.exit(1); }
console.log("alle Tests grün");
