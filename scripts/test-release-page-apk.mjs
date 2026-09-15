#!/usr/bin/env node
// Die Release-Seite bietet die Tablet-APK an — aber nur, was wirklich auf dem
// Server liegt, und der feste Name `bts-light-tablet.apk` nur, wenn eine
// SIGNIERTE APK existiert (docs/release.md). Geprüft wird der echte Generator
// über die Kommandozeile mit einem Changelog-Fixture und --apks-Listen.
//
// Aufruf:  node scripts/test-release-page-apk.mjs
// Exit:    0 = ok, 1 = mind. eine Prüfung fehlgeschlagen.

import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const tmp = mkdtempSync(join(tmpdir(), "bts-apk-"));
const changelog = join(tmp, "changelog.md");
writeFileSync(
  changelog,
  `# Änderungsverlauf

## v0.9.285

- **Tablet-App: Einrichtung entschlackt.** Erster Punkt.

## v0.9.284

- **Zähl-Tablet: größeres Feld.** Zweiter Punkt.

## v0.9.283

- **Tablet-Kiosk-App.** Dritter Punkt.
`
);

let fehler = 0;
const pruefe = (ok, text) => {
  console.log(`  ${ok ? "✓" : "✗"} ${text}`);
  if (!ok) fehler++;
};

function seite(apks) {
  const out = join(tmp, `index-${Math.random().toString(36).slice(2)}.html`);
  const args = ["scripts/build-release-page.mjs", "--changelog", changelog, "--out", out];
  if (apks !== null) {
    const f = join(tmp, "apks.txt");
    writeFileSync(f, apks.join("\n") + "\n");
    args.push("--apks", f);
  }
  execFileSync(process.execPath, args, { stdio: ["ignore", "ignore", "inherit"] });
  return readFileSync(out, "utf8");
}

console.log("Ohne --apks: kein APK-Abschnitt, kein APK-Knopf");
{
  const html = seite(null);
  pruefe(!html.includes('id="tablet-apk"'), "kein Abschnitt");
  pruefe(!html.includes("Tablet-APK"), "kein Knopf je Version");
  pruefe(!html.includes("bts-light-tablet"), "kein APK-Link");
}

console.log("Nur Debug-APKs: neueste versionierte Datei, fester Name NICHT verlinkt");
{
  const html = seite(["bts-light-tablet-0.9.283-debug.apk", "bts-light-tablet-0.9.285-debug.apk", "bts-light-tablet-0.9.284-debug.apk"]);
  pruefe(html.includes('id="tablet-apk"'), "Abschnitt vorhanden");
  pruefe(html.includes('href="bts-light-tablet-0.9.285-debug.apk">APK herunterladen (v0.9.285, Debug)'), "neueste Debug-APK verlinkt");
  pruefe(!html.includes('href="bts-light-tablet.apk"'), "fester Name nicht verlinkt");
  pruefe(html.includes("trägt eine Debug-Signatur"), "Hinweis auf Debug-Signatur im Abschnitt");
  pruefe(html.includes('href="#tablet-apk"'), "Sprungmarke im Kopf");
  pruefe(html.includes('href="bts-light-tablet-0.9.284-debug.apk"'), "APK-Knopf bei v0.9.284");
  pruefe((html.match(/Tablet-APK \(Debug\)/g) || []).length === 3, "je Version ein Debug-Knopf");
}

console.log("Signierte APK vorhanden: fester Name im Abschnitt, signiert vor Debug je Version");
{
  const html = seite(["bts-light-tablet-0.9.285-debug.apk", "bts-light-tablet-0.9.285.apk", "bts-light-tablet-0.9.284-debug.apk"]);
  pruefe(html.includes('href="bts-light-tablet.apk">APK herunterladen (v0.9.285)'), "fester Name verlinkt, ohne Debug-Zusatz");
  pruefe(!html.includes("trägt eine Debug-Signatur"), "kein Debug-Hinweis im Abschnitt");
  pruefe(html.includes('href="bts-light-tablet-0.9.285.apk"'), "signierte APK bei v0.9.285");
  pruefe(!html.includes('href="bts-light-tablet-0.9.285-debug.apk"'), "Debug-APK bei v0.9.285 nicht angeboten, wenn signiert da");
  pruefe(html.includes('href="bts-light-tablet-0.9.284-debug.apk"'), "v0.9.284 weiter mit Debug-APK");
}

console.log("Ältere signierte + neuere Debug-APK: der Kopf nimmt die signierte (fester Name stimmt)");
{
  const html = seite(["bts-light-tablet-0.9.284.apk", "bts-light-tablet-0.9.285-debug.apk"]);
  pruefe(html.includes('href="bts-light-tablet.apk">APK herunterladen (v0.9.284)'), "fester Name mit Etikett der signierten Version");
  pruefe(!html.includes("trägt eine Debug-Signatur"), "kein Debug-Hinweis im Abschnitt");
  pruefe(/href="bts-light-tablet-0\.9\.284\.apk"[^>]*>Tablet-APK<\/a>/.test(html), "v0.9.284 signiert");
  pruefe(/href="bts-light-tablet-0\.9\.285-debug\.apk"[^>]*>Tablet-APK \(Debug\)<\/a>/.test(html), "v0.9.285 Debug");
}

console.log("Dieselbe Datei doppelt (Server + frisch gebaut): nichts doppelt gerendert");
{
  const html = seite(["bts-light-tablet-0.9.285-debug.apk", "bts-light-tablet-0.9.285-debug.apk"]);
  pruefe((html.match(/Tablet-APK \(Debug\)/g) || []).length === 1, "ein Knopf");
  pruefe((html.match(/id="tablet-apk"/g) || []).length === 1, "ein Abschnitt");
}

console.log("Fremde Dateinamen werden ignoriert");
{
  const html = seite(["bts-light-tablet.apk", "irgendwas.apk", "bts-light-tablet-abc-debug.apk"]);
  pruefe(!html.includes('id="tablet-apk"'), "kein Abschnitt ohne versionierte APK");
}

if (fehler) {
  console.error(`${fehler} Prüfung(en) fehlgeschlagen`);
  process.exit(1);
}
console.log("alle Prüfungen bestanden");
