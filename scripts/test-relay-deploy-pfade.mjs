// Prüft, dass jede Datei (und jedes Verzeichnis), die das Relay per
// `include_str!`/`include_bytes!`/`include_dir!` einbettet, in den `paths` von `.github/workflows/relay-deploy.yml` steht.
//
// Warum: Fehlt eine eingebettete Seite dort, löst eine Änderung an ihr
// keinen Relay-Deploy aus — sie wirkt im LAN, bleibt in der Cloud aber
// unsichtbar, bis zufällig ein anderer Deploy läuft. Viermal passiert
// (zuletzt tafel/stream/anzeige/meetingpoint bis 02.11.2026).
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** Alle .rs-Dateien unter einem Verzeichnis. */
function rustDateien(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (name !== "target") out.push(...rustDateien(p));
    } else if (name.endsWith(".rs")) out.push(p);
  }
  return out;
}

/** Repo-relative Pfade aller Einbettungen (nur die außerhalb der Crates —
 *  was in `relay/**` oder `relay-proto/**` liegt, deckt das Glob ab). */
function einbettungen() {
  const pfade = new Set();
  for (const crate of ["relay/src", "relay-proto/src"]) {
    for (const datei of rustDateien(join(root, crate))) {
      const text = readFileSync(datei, "utf8");
      // include_str!/include_bytes!: relativ zur enthaltenden .rs-Datei.
      for (const m of text.matchAll(/include_(?:str|bytes)!\(\s*"([^"]+)"\s*\)/g)) {
        pfade.add(relative(root, resolve(dirname(datei), m[1])).split("\\").join("/"));
      }
      // include_dir! (Crate include_dir): relativ zu $CARGO_MANIFEST_DIR;
      // ein Verzeichnis zählt als gedeckt, wenn `<dir>/**` in den paths steht.
      const crateDir = join(root, crate.split("/")[0]);
      for (const m of text.matchAll(/include_dir!\(\s*"\$CARGO_MANIFEST_DIR\/([^"]+)"\s*\)/g)) {
        pfade.add(relative(root, resolve(crateDir, m[1])).split("\\").join("/") + "/**");
      }
      if (/include!\(|include_dir!\((?!\s*"\$CARGO_MANIFEST_DIR\/)/.test(text)) {
        throw new Error(`${relative(root, datei)}: include!/include_dir! in unbekannter Form — Prüfung erweitern`);
      }
    }
  }
  return [...pfade];
}

/** Die `paths`-Einträge des push-Auslösers im Workflow. */
function workflowPfade() {
  const yml = readFileSync(join(root, ".github/workflows/relay-deploy.yml"), "utf8");
  const block = yml.split(/\n\s*paths:\s*\n/)[1]?.split(/\n\S|\n\s*\n\s*jobs:/)[0] ?? "";
  return [...block.matchAll(/^\s*-\s*"([^"]+)"/gm)].map((m) => m[1]);
}

/** Deckt ein Workflow-Pfad (ggf. mit `/**`) die Datei ab? */
function gedeckt(datei, muster) {
  if (datei.endsWith("/**")) return muster.includes(datei) || muster.some((p) => p.endsWith("/**") && datei.startsWith(p.slice(0, -2)));
  return muster.some((p) =>
    p.endsWith("/**") ? datei.startsWith(p.slice(0, -2)) : p === datei,
  );
}

const muster = workflowPfade();
const alle = einbettungen();
let fehler = 0;
if (muster.length === 0) {
  console.error("FEHLER: keine paths in relay-deploy.yml gefunden (Format geändert?)");
  fehler++;
}
if (alle.length === 0) {
  console.error("FEHLER: keine Einbettungen im Relay gefunden (Format geändert?)");
  fehler++;
}
for (const datei of alle) {
  if (!gedeckt(datei, muster)) {
    console.error(`FEHLER: ${datei} ist ins Relay eingebettet, fehlt aber in den paths von .github/workflows/relay-deploy.yml`);
    fehler++;
  }
}
if (fehler) process.exit(1);
console.log(`ok: alle ${alle.length} eingebetteten Dateien lösen den Relay-Deploy aus`);
