#!/usr/bin/env node
// Update-Notes decken die ganze Strecke ab, nicht nur den getaggten Commit.
//
// WARUM DAS ZÄHLT: Zwischen zwei Release-Tags liegen in diesem Repo
// regelmäßig mehrere Versionssprünge — v0.9.214 → v0.9.223 waren neun. Wer
// aktualisiert, springt genau über diese Strecke. Bis zum 2026-08-18 zeigte
// das Update-Fenster trotzdem nur den Changelog-Abschnitt der getaggten
// Version: Acht Änderungen (Push-Kanal statt Poll, Panel „Anfangszeiten",
// Schriftgröße pro Gerät …) waren ausgeliefert, aber für den Nutzer
// unsichtbar. Kein Fehler, den irgendetwas gemeldet hätte — die Datei war
// ja gültig.
//
// Geprüft wird der echte Generator über die Kommandozeile, mit einem
// Changelog-Fixture statt docs/changelog.md, damit der Test nicht bei jedem
// neuen Eintrag umkippt.
//
// Aufruf:  node scripts/test-release-notes.mjs
// Exit:    0 = ok, 1 = mind. eine Prüfung fehlgeschlagen.

import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const tmp = mkdtempSync(join(tmpdir(), "bts-notes-"));
const changelog = join(tmp, "changelog.md");

writeFileSync(
  changelog,
  `# Änderungsverlauf

## v0.9.223

- **Ein lahmes badhub kann keine Tablets mehr abwerfen.** Der Live-Score-Push
  lief bisher innerhalb der Tablet-Verbindung.
- **Nachschub nach BTP hält den Zyklus nicht auf.** Zweiter Punkt.

## v0.9.222

- **Config-Cache.** Nicht mehr bei jeder Anfrage von der Platte lesen.

## v0.9.221

- **Push-Kanal statt Poll.** TL-Web wird geweckt, statt zu fragen.

## v0.9.214

- **Spielliste ohne Hinweistexte.** Farben, Eieruhr, Spieler-Links.
`
);

let fehler = 0;
const pruefe = (ok, text) => {
  console.log(`  ${ok ? "✓" : "✗"} ${text}`);
  if (!ok) fehler++;
};

function notes(args) {
  const out = join(tmp, "notes.txt");
  execFileSync(
    process.execPath,
    ["scripts/build-release-page.mjs", "--changelog", changelog,
     "--out", join(tmp, "index.html"), "--notes-out", out, ...args],
    { stdio: ["ignore", "ignore", "pipe"] }
  );
  return readFileSync(out, "utf8");
}

console.log("Update-Notes über die Release-Strecke");

// ── 1. Der Regressionsfall: Strecke über mehrere Versionen ────────────────
const strecke = notes(["--notes-version", "0.9.223", "--notes-since", "0.9.214"]);
pruefe(strecke.includes("0.9.223"), "Strecke nennt die getaggte Version");
pruefe(strecke.includes("0.9.222"), "Strecke nennt die übersprungene 0.9.222");
pruefe(strecke.includes("0.9.221"), "Strecke nennt die übersprungene 0.9.221");
pruefe(!strecke.includes("0.9.214"), "die BEREITS veröffentlichte 0.9.214 fehlt (since ist exklusiv)");
pruefe(/^Dieses Update fasst 3 Versionen/.test(strecke), "Kopfzeile nennt die Anzahl");
// Reihenfolge im RUMPF pruefen, nicht in der Kopfzeile: die nennt die
// Spanne "v0.9.221 – v0.9.223" und wuerde die Suche verfaelschen.
const rumpf = strecke.split("\n").slice(2);
const reihenfolge = rumpf.filter((z) => /^v0\.9\.\d+$/.test(z));
pruefe(
  JSON.stringify(reihenfolge) === JSON.stringify(["v0.9.223", "v0.9.222", "v0.9.221"]),
  "Versionen stehen neueste zuerst: " + reihenfolge.join(", ")
);

// ── 2. Ohne --notes-since: unverändertes Altverhalten ─────────────────────
const einzeln = notes(["--notes-version", "0.9.223"]);
pruefe(!einzeln.includes("0.9.222"), "ohne --notes-since nur die getaggte Version");
pruefe(
  einzeln.startsWith("• "),
  "einzelne Version bleibt die reine Stichpunktliste (keine Kopfzeile)"
);
pruefe(
  einzeln.includes("Nachschub nach BTP"),
  "einzelne Version zeigt ALLE ihre Stichpunkte"
);

// ── 3. Länge: der Dialog bleibt lesbar ───────────────────────────────────
const lang = join(tmp, "lang.md");
let md = "# Änderungsverlauf\n\n";
for (let n = 240; n >= 200; n--) {
  md += `## v0.9.${n}\n\n- **Punkt ${n}.** ${"Sehr ausführliche Begründung. ".repeat(12)}\n\n`;
}
writeFileSync(lang, md);
execFileSync(
  process.execPath,
  ["scripts/build-release-page.mjs", "--changelog", lang, "--out", join(tmp, "i2.html"),
   "--notes-out", join(tmp, "n2.txt"), "--notes-version", "0.9.240", "--notes-since", "0.9.200"],
  { stdio: ["ignore", "ignore", "pipe"] }
);
const gekuerzt = readFileSync(join(tmp, "n2.txt"), "utf8");
pruefe(gekuerzt.length <= 4000, `gekürzte Fassung bleibt unter 4000 Zeichen (${gekuerzt.length})`);
pruefe(
  gekuerzt.split("\n").filter((z) => z.startsWith("• v0.9.")).length === 40,
  "gekürzte Fassung behält je Version eine Zeile"
);
pruefe(
  gekuerzt.includes("badhub.de/download/bts-light/"),
  "gekürzte Fassung verweist auf die vollständige Release-Seite"
);

// ── 4. Fehlender Changelog-Abschnitt bricht den Release nicht ────────────
const fehlend = notes(["--notes-version", "0.9.999"]);
pruefe(fehlend.trim() === "BTS Light 0.9.999", "unbekannte Version → generischer Einzeiler statt Absturz");

// ── 5. Unbrauchbares --notes-since verliert die Notes NICHT ──────────────
//
// Der gefaehrliche Fall ist nicht "zu wenig Text", sondern ein LEERER
// Bereich: Dann faende der Generator die getaggte Version nicht mehr und
// schriebe den generischen Einzeiler — die Stichpunkte waeren weg, obwohl
// sie im Changelog stehen. Drei Wege dorthin, alle abgefangen:
for (const [wert, was] of [
  ["0.9.223", "since == version"],
  ["0.9.999", "since > version"],
  ["kein-tag", "since ist gar keine Version"],
]) {
  const r = notes(["--notes-version", "0.9.223", "--notes-since", wert]);
  pruefe(
    r.includes("Ein lahmes badhub") && r.includes("Nachschub nach BTP"),
    `${was} → Rückfall auf die getaggte Version, Stichpunkte bleiben`
  );
  pruefe(!r.includes("0.9.221"), `${was} → keine fremden Versionen im Text`);
}

// ── 6. Stichpunkt ohne **fett** und ohne Satzende ────────────────────────
const roh = join(tmp, "roh.md");
let md2 = "# Änderungsverlauf\n\n";
for (let n = 240; n >= 210; n--) {
  md2 += `## v0.9.${n}\n\n- Ganz ohne Fettdruck und ohne Satzzeichen ${"und weiter ".repeat(40)}\n\n`;
}
writeFileSync(roh, md2);
execFileSync(
  process.execPath,
  ["scripts/build-release-page.mjs", "--changelog", roh, "--out", join(tmp, "i3.html"),
   "--notes-out", join(tmp, "n3.txt"), "--notes-version", "0.9.240", "--notes-since", "0.9.210"],
  { stdio: ["ignore", "ignore", "pipe"] }
);
const ohneFett = readFileSync(join(tmp, "n3.txt"), "utf8");
pruefe(ohneFett.length <= 4000, `ohne Fettdruck trotzdem unter 4000 Zeichen (${ohneFett.length})`);
pruefe(
  ohneFett.split("\n").every((z) => z.length <= 120),
  "keine Zeile laeuft aus dem Dialog (je Zeile gekappt)"
);

// ── 7. Sehr viele Versionen: auch die Kurzfassung bleibt endlich ─────────
const viele = join(tmp, "viele.md");
let md3 = "# Änderungsverlauf\n\n";
for (let n = 400; n >= 100; n--) {
  md3 += `## v0.9.${n}\n\n- **Punkt ${n} mit einer ausreichend langen Kernaussage fuer die Messung.**\n\n`;
}
writeFileSync(viele, md3);
execFileSync(
  process.execPath,
  ["scripts/build-release-page.mjs", "--changelog", viele, "--out", join(tmp, "i4.html"),
   "--notes-out", join(tmp, "n4.txt"), "--notes-version", "0.9.400", "--notes-since", "0.9.100"],
  { stdio: ["ignore", "ignore", "pipe"] }
);
const sehrViele = readFileSync(join(tmp, "n4.txt"), "utf8");
pruefe(sehrViele.length <= 4000, `300 Versionen bleiben unter 4000 Zeichen (${sehrViele.length})`);
pruefe(
  sehrViele.trimEnd().endsWith("badhub.de/download/bts-light/"),
  "auch die harte Kappung endet mit dem Verweis auf die Release-Seite"
);

// Die Release-Seite verweist auf das Pi-Image der Court-Monitore.
//
// WARUM DAS ZÄHLT: Die SD-Karten werden beim Hallenaufbau beschrieben, oft von
// Leuten ohne das Repo. Stand der Link nur in docs/pi-dual-image.md, war er
// genau dann nicht erreichbar, wenn er gebraucht wurde. Der Block wird beim
// Release mitgeneriert — faellt er still weg (Umbau am Seitengerüst), merkt es
// niemand, bis wieder jemand in einer Halle danach sucht.
const seite = readFileSync(join(tmp, "index.html"), "utf8");
pruefe(
  seite.includes('href="pi-image/bts-light-pi.img.xz"'),
  "Release-Seite verlinkt das Pi-Image"
);
pruefe(
  seite.includes('href="pi-image/bts-light-pi.img.xz.sha256"'),
  "Release-Seite verlinkt die Pruefsumme des Pi-Images"
);
pruefe(
  seite.includes('href="#pi-image"') && seite.includes('id="pi-image"'),
  "Kopf-Knopf neben dem Programm-Download springt zum Pi-Image-Block"
);

console.log(fehler === 0 ? "\nOK" : `\n${fehler} fehlgeschlagen`);
process.exit(fehler === 0 ? 0 : 1);
