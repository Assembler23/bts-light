// Syntax-Prüfung der Inline-Skripte in den Assets (tl.html, tablet.html …).
//
// Diese Seiten durchlaufen **keinen** Build: Sie werden so, wie sie im Repo
// stehen, an Browser und Tablets ausgeliefert. Ein Tippfehler fällt deshalb
// erst im Betrieb auf — und zwar hart, weil ein Syntaxfehler das komplette
// Modul unausgeführt lässt und die Seite dann leer bleibt. Genau das ist am
// 13.08.2026 passiert (ein Zeilenumbruch mitten in einem String-Literal in
// tl.html), deshalb prüft die CI das jetzt.
//
// Geprüft wird nur die **Syntax**: Die Skripte werden importiert, und alles
// außer einem `SyntaxError` gilt als bestanden (ein `document`-Zugriff ohne
// Browser wirft erwartungsgemäß einen `ReferenceError`).
import { readFileSync, writeFileSync, mkdtempSync } from "node:fs";
import { readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const VERZEICHNIS = "src-tauri/assets";
const dateien = readdirSync(VERZEICHNIS)
  .filter((f) => f.endsWith(".html"))
  .map((f) => join(VERZEICHNIS, f));

const tmp = mkdtempSync(join(tmpdir(), "asset-syntax-"));
let fehler = 0;
let geprueft = 0;

for (const datei of dateien) {
  const html = readFileSync(datei, "utf8");
  const bloecke = [...html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)];
  let nr = 0;
  for (const [, attrs, code] of bloecke) {
    if (/\bsrc=/.test(attrs) || code.trim().length === 0) continue;
    nr += 1;
    geprueft += 1;
    const pfad = join(tmp, `${datei.replace(/[^a-z0-9]/gi, "_")}-${nr}.mjs`);
    writeFileSync(pfad, code, "utf8");
    try {
      await import(`file://${pfad.replace(/\\/g, "/")}`);
    } catch (e) {
      if (e instanceof SyntaxError) {
        fehler += 1;
        console.error(`FEHLER ${datei} (Skript ${nr}): ${e.message}`);
      }
      // Alles andere ist ein Laufzeitfehler ohne Browser — die Syntax stimmt.
    }
  }
}

if (fehler > 0) {
  console.error(`\n${fehler} Skript(e) mit Syntaxfehler.`);
  process.exit(1);
}
console.log(`Asset-Syntax ok (${geprueft} Inline-Skript(e) in ${dateien.length} Dateien).`);
