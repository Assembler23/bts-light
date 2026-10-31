// Testet die Disziplin-Benennung (src/io/disziplinName.mjs): Jugendklassen
// („U15") heißen Jungen-/Mädchen-Disziplin, alles andere wie bisher. Dazu
// die Inline-Kopien in tl.html und monitor.html — sie müssen wortgleich
// bleiben, sonst nennt der Monitor „Herreneinzel", wo die Ansage
// „Jungeneinzel" sagt.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { disziplinName } from "../src/io/disziplinName.mjs";

let failures = 0;
function eq(name, got, want) {
  const g = JSON.stringify(got);
  const w = JSON.stringify(want);
  if (g !== w) {
    console.error(`✗ ${name}: erwartet ${w}, war ${g}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

// Erwachsene: unverändert.
eq("HE ohne Klasse", disziplinName("mens_singles", ""), { kurz: "HE", de: "Herreneinzel", en: "Men's Singles" });
eq("DD Klasse A", disziplinName("womens_doubles", "A").kurz, "DD");
eq("Mixed bleibt GD", disziplinName("mixed", "C").kurz, "GD");
// Seniorenklassen sind keine Jugend.
eq("HE O35", disziplinName("mens_singles", "O35").kurz, "HE");
eq("HE U-ähnlich, aber mehr", disziplinName("mens_singles", "U15A").kurz, "HE");

// Jugend (DBV-Jugendturnier 10/2026: „JE U15", „MD U17", „MX U13").
eq("JE U15", disziplinName("mens_singles", "U15"), { kurz: "JE", de: "Jungeneinzel", en: "Boys' Singles" });
eq("ME U11", disziplinName("womens_singles", "U11"), { kurz: "ME", de: "Mädcheneinzel", en: "Girls' Singles" });
eq("JD U19", disziplinName("mens_doubles", "U19").de, "Jungendoppel");
eq("MD U17", disziplinName("womens_doubles", "U17").kurz, "MD");
eq("MX U13", disziplinName("mixed", "U13").kurz, "MX");
eq("klein geschrieben", disziplinName("mens_singles", " u9 ").kurz, "JE");

// Unbekannt → leer, nie „undefined".
eq("unknown", disziplinName("unknown", "U15"), { kurz: "", de: "", en: "" });
eq("fehlend", disziplinName(undefined, undefined), { kurz: "", de: "", en: "" });

// Inline-Kopien wortgleich (Einrückung + Anführungszeichen normalisiert,
// gleiches Muster wie scripts/test-abruf-frist.mjs).
{
  const root = join(dirname(fileURLToPath(import.meta.url)), "..");
  const normal = (txt) => {
    const tiefe = txt.match(/^ */)[0].length;
    return txt
      .split("\n")
      .map((z) => z.slice(Math.min(tiefe, z.match(/^ */)[0].length)))
      .join("\n")
      .trim();
  };
  const finde = (txt) => txt.replace(/\r\n/g, "\n").match(/ *(?:export )?function disziplinName\(key, klasse\) \{[\s\S]*?\n {0,2}\}\n/);
  const modul = finde(readFileSync(join(root, "src", "io", "disziplinName.mjs"), "utf8"));
  const referenz = normal(modul[0].replace("export ", ""));
  for (const seite of ["tl.html", "monitor.html"]) {
    const m = finde(readFileSync(join(root, "src-tauri", "assets", seite), "utf8"));
    eq(`${seite}: trägt die Kopie`, !!m, true);
    if (m) eq(`${seite}: Kopie gleicht dem Modul`, normal(m[0]) === referenz, true);
  }
}

if (failures) {
  console.error(`${failures} Test(s) fehlgeschlagen`);
  process.exit(1);
}
console.log("Alle Disziplin-Namen-Tests grün.");
