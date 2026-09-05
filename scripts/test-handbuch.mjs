#!/usr/bin/env node
// Das öffentliche Handbuch veröffentlicht genau das, was es soll — und nichts sonst.
//
// WARUM DAS ZÄHLT: docs/handbuch.json entscheidet, welche Repo-Dokumentation auf
// badhub.de landet. Zwei Fehler wären dabei still:
//   1. Ein interner Abschnitt wird wieder öffentlich, weil jemand seine
//      Überschrift umbenannt hat und der "aus"-Eintrag ins Leere greift.
//      Nichts würde das melden — die Seite sähe nur etwas länger aus.
//   2. Eine neue Anleitung unter docs/ bleibt jahrelang unveröffentlicht,
//      weil niemand daran gedacht hat, sie einzutragen.
// Beides prüft dieser Test. Zusätzlich: keine toten Querverweise (die Doku
// verlinkt sich quer, und nicht jede Zieldatei ist veröffentlicht).
//
// Aufruf:  node scripts/test-handbuch.mjs
// Exit:    0 = ok, 1 = mind. eine Prüfung fehlgeschlagen.

import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, readdirSync, existsSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

let fehler = 0;
const pruefe = (ok, text) => {
  console.log(`  ${ok ? "✓" : "✗"} ${text}`);
  if (!ok) fehler++;
};

const tmp = mkdtempSync(join(tmpdir(), "bts-handbuch-"));
const manifest = JSON.parse(readFileSync("docs/handbuch.json", "utf8"));
const seiten = manifest.gruppen.flatMap((g) => g.seiten);
const inhalt = seiten.filter((s) => s.datei);

// ── 1. Manifest ist in sich stimmig ───────────────────────────────────────
console.log("\nManifest");

for (const s of inhalt) {
  pruefe(existsSync(s.datei), `Quelldatei existiert: ${s.datei}`);
}
const slugs = inhalt.map((s) => s.slug);
pruefe(new Set(slugs).size === slugs.length, "Slugs sind eindeutig (sonst überschreiben sich Seiten)");
pruefe(!slugs.includes("index"), "kein Kapitel heißt 'index' (das ist die Startseite)");
for (const s of inhalt) {
  pruefe(/^[a-z0-9-]+$/.test(s.slug || ""), `Slug ist dateisystem- und URL-tauglich: ${s.slug}`);
}

// ── 2. Keine Anleitung bleibt unbemerkt liegen ────────────────────────────
//
// Jede Markdown-Datei unter docs/ muss eine Entscheidung haben: veröffentlicht
// (im Manifest) oder bewusst intern ("intern"). Wer eine neue Anleitung
// schreibt, wird hier daran erinnert, statt dass sie unsichtbar bleibt.
console.log("\nVollständigkeit");

const veroeffentlicht = new Set(inhalt.map((s) => s.datei));
const internPraefixe = manifest.intern || [];
const istIntern = (p) => internPraefixe.some((i) => (i.endsWith("/") ? p.startsWith(i) : p === i));

// REKURSIV: docs/ hat Unterverzeichnisse (adr/, features/, superpowers/) mit
// dem Grossteil der Dateien — 137 insgesamt, nur 32 auf oberster Ebene. Ein
// Check, der nur die oberste Ebene sieht, verspricht eine Garantie, die er
// fuer drei Viertel des Baums nicht einloest (Review 05.09.2026).
const alleDocs = readdirSync("docs", { recursive: true })
  .map((f) => `docs/${String(f).split("\\").join("/")}`)
  .filter((p) => p.endsWith(".md"));
// Selbstkontrolle: Ein gruener Vollstaendigkeits-Check ist wertlos, wenn die
// Liste leer oder verkuerzt ist. Genau so entstand der Befund vom 05.09.2026 —
// die Pruefung lief gruen, sah aber nur ein Viertel der Dateien.
pruefe(
  alleDocs.some((p) => p.slice("docs/".length).includes("/")),
  "die Suche steigt in Unterverzeichnisse ab (adr/, features/ …)"
);
pruefe(alleDocs.length > alleDocs.filter((p) => !p.slice("docs/".length).includes("/")).length,
  `mehr Dateien als nur die oberste Ebene gesehen (${alleDocs.length})`);

const unentschieden = alleDocs.filter((p) => !veroeffentlicht.has(p) && !istIntern(p));
pruefe(
  unentschieden.length === 0,
  unentschieden.length === 0
    ? "jede docs/*.md ist entweder veröffentlicht oder als intern eingetragen"
    : `unentschieden (ins Manifest oder unter 'intern' eintragen): ${unentschieden.join(", ")}`
);

// ── 3. Der Generator läuft und meldet umbenannte Abschnitte ───────────────
console.log("\nAbschnitts-Ausschluss");

const fixtureDir = join(tmp, "fix");
mkdirSync(join(fixtureDir, "docs"), { recursive: true });
writeFileSync(
  join(fixtureDir, "docs", "a.md"),
  `# Titel

## Bedienung

Sichtbarer Text.

### Unterpunkt der Bedienung

Auch sichtbar.

## Interner Kram

GEHEIMNIS-EINS

### Noch tiefer

GEHEIMNIS-ZWEI

## Danach

Wieder sichtbar.

<!-- handbuch:aus -->
GEHEIMNIS-DREI
<!-- handbuch:an -->

Ende.
`
);
const fixManifest = (aus) => {
  const p = join(fixtureDir, "m.json");
  writeFileSync(
    p,
    JSON.stringify({
      titel: "T",
      untertitel: "U",
      gruppen: [{ name: "G", seiten: [{ datei: "docs/a.md", slug: "a", titel: "A", aus }] }],
      intern: [],
    })
  );
  return p;
};
function baue(aus, ziel) {
  return execFileSync(
    process.execPath,
    ["scripts/build-handbuch.mjs", "--manifest", fixManifest(aus), "--basis", fixtureDir, "--out", ziel],
    { stdio: ["ignore", "ignore", "pipe"], encoding: "utf8" }
  );
}

const ziel1 = join(tmp, "out1");
baue(["Interner Kram"], ziel1);
const a1 = readFileSync(join(ziel1, "a.html"), "utf8");
pruefe(!a1.includes("GEHEIMNIS-EINS"), "ausgeschlossener Abschnitt fehlt in der Ausgabe");
pruefe(!a1.includes("GEHEIMNIS-ZWEI"), "seine UNTERabschnitte fliegen mit raus (sonst bliebe das Innere stehen)");
pruefe(!a1.includes("GEHEIMNIS-DREI"), "Marker-Bereich handbuch:aus … :an fehlt in der Ausgabe");
pruefe(a1.includes("Sichtbarer Text"), "der Rest der Seite bleibt erhalten");
pruefe(a1.includes("Wieder sichtbar"), "der Ausschluss endet an der nächsten gleichrangigen Überschrift");
pruefe(a1.includes("Auch sichtbar"), "Unterabschnitte eines BEHALTENEN Abschnitts bleiben");

// Der wichtigste Fall: Überschrift umbenannt → der Ausschluss greift ins Leere.
let brach = false;
let meldung = "";
try {
  baue(["Interner Kram — umbenannt"], join(tmp, "out2"));
} catch (e) {
  brach = true;
  meldung = String(e.stderr || "");
}
pruefe(brach, "umbenannte Überschrift lässt den Build FEHLSCHLAGEN, statt still zu veröffentlichen");
pruefe(/existiert nicht/.test(meldung), "die Fehlermeldung nennt den Grund");

// Offener Marker ist ebenfalls ein Fehler — sonst verschluckt ein vergessenes
// :an den halben Rest der Seite, ohne dass es auffällt.
writeFileSync(
  join(fixtureDir, "docs", "a.md"),
  `# T\n\n## X\n\n<!-- handbuch:aus -->\nweg\n\nund weiter weg\n`
);
let brach2 = false;
try {
  baue([], join(tmp, "out3"));
} catch {
  brach2 = true;
}
pruefe(brach2, "nie geschlossenes 'handbuch:aus' lässt den Build fehlschlagen");

// Setext-Ueberschriften ("Titel" + "====" darunter) sind nach CommonMark
// Ueberschriften und werden von marked auch so gerendert. Wuerde die
// Abschnittserkennung sie uebersehen, liefe ein Ausschluss ueber sie hinweg
// und naehme sichtbare Kapitel mit — stiller Textverlust ohne roten Test.
writeFileSync(
  join(fixtureDir, "docs", "a.md"),
  `# Titel

## Interner Kram

GEHEIMNIS-VIER

Sichtbar per Setext
===================

BLEIBT-STEHEN

Auch als H2
-----------

BLEIBT-AUCH
`
);
const ziel4 = join(tmp, "out4");
baue(["Interner Kram"], ziel4);
const a4 = readFileSync(join(ziel4, "a.html"), "utf8");
pruefe(!a4.includes("GEHEIMNIS-VIER"), "Setext: der ausgeschlossene Abschnitt fehlt");
pruefe(a4.includes("BLEIBT-STEHEN"), "Setext-H1 beendet den Ausschluss (kein stiller Textverlust)");
pruefe(a4.includes("BLEIBT-AUCH"), "Setext-H2 beendet den Ausschluss ebenfalls");

// Ein Link, der per ../ aus dem Repo ausbricht, darf niemals als Verweis
// ueberleben — er wird zu reinem Text entwertet (fail-closed).
writeFileSync(
  join(fixtureDir, "docs", "a.md"),
  `# T

## X

[Ausbruch](../../../../etc/passwd.md) und [Intern](adr/0001-quality-gate.md).
`
);
const ziel5 = join(tmp, "out5");
baue([], ziel5);
const a5 = readFileSync(join(ziel5, "a.html"), "utf8");
pruefe(!/<a [^>]*href="[^"]*passwd/.test(a5), "Pfad-Ausbruch per ../ wird zu Text entwertet, nicht verlinkt");
pruefe(a5.includes("Ausbruch"), "der Linktext bleibt dabei lesbar erhalten");
pruefe(!/<a [^>]*href="[^"]*0001-quality-gate/.test(a5), "Link auf eine nicht veröffentlichte Datei wird entwertet");

// ── 4. Das echte Handbuch bauen und die Ausgabe prüfen ────────────────────
console.log("\nEchter Lauf");

const echt = join(tmp, "handbuch");
let lief = true;
try {
  execFileSync(process.execPath, ["scripts/build-handbuch.mjs", "--out", echt], {
    stdio: ["ignore", "ignore", "pipe"],
  });
} catch (e) {
  lief = false;
  console.log(String(e.stderr || ""));
}
pruefe(lief, "das echte Handbuch baut ohne Fehler");

if (lief) {
  const dateien = readdirSync(echt).filter((f) => f.endsWith(".html"));
  pruefe(dateien.length === inhalt.length + 1, `${inhalt.length} Kapitel + Startseite erzeugt (${dateien.length})`);

  // Anker je Seite einsammeln, dann alle Querverweise dagegen prüfen. Ein
  // Verweis auf "#feldvergabe" in einer Datei, in der dieser Abschnitt
  // ausgeschlossen wurde, führt sonst ins Nichts.
  const anker = new Map();
  const html = new Map();
  for (const f of dateien) {
    const t = readFileSync(join(echt, f), "utf8");
    html.set(f, t);
    anker.set(f, new Set([...t.matchAll(/<h[2-4] id="([^"]+)"/g)].map((m) => m[1])));
  }

  const tot = [];
  for (const [f, t] of html) {
    for (const m of t.matchAll(/href="([^"]+)"/g)) {
      const href = m[1];
      if (/^(https?:|mailto:|\.\.\/)/i.test(href)) continue;
      // marked prozentkodiert Umlaute im Anker (ü → %C3%BC); Browser lösen das
      // vor dem Sprung auf, der Vergleich hier muss es auch tun.
      const [datei, ankRoh] = href.split("#");
      let ank = ankRoh;
      try { ank = ankRoh ? decodeURIComponent(ankRoh) : ankRoh; } catch { /* kaputte Kodierung: roh vergleichen */ }
      const zielDatei = datei === "" ? f : datei;
      if (!html.has(zielDatei)) tot.push(`${f} → ${href} (Seite fehlt)`);
      else if (ank && !anker.get(zielDatei).has(ank)) tot.push(`${f} → ${href} (Anker fehlt)`);
    }
  }
  pruefe(tot.length === 0, tot.length === 0 ? "kein toter Querverweis" : `tote Verweise:\n     ${tot.slice(0, 12).join("\n     ")}${tot.length > 12 ? `\n     … und ${tot.length - 12} weitere` : ""}`);

  // Kein .md-Link überlebt: relative Doku-Links laufen auf der Website ins Leere.
  const mdLinks = [...html].filter(([, t]) => /href="[^"]*\.md[^"]*"/.test(t)).map(([f]) => f);
  pruefe(mdLinks.length === 0, mdLinks.length === 0 ? "kein Link zeigt auf eine .md-Datei" : `.md-Links in: ${mdLinks.join(", ")}`);

  // Betriebsgeheimnisse des Servers gehören nicht in ein Endnutzer-Handbuch.
  // Der Pi hat eine eigene sudoers-Datei (Image-Bau) — die ist gemeint und erlaubt,
  // deshalb wird auf die badhub-spezifischen Zeichen geprüft, nicht auf "sudo".
  const verboten = ["178.104.221.177", "bts-deploy", "SSH_DEPLOY", "/var/www/badhub", "sudoers.d/bts"];
  const leaks = [];
  for (const [f, t] of html) for (const v of verboten) if (t.includes(v)) leaks.push(`${f}: ${v}`);
  pruefe(leaks.length === 0, leaks.length === 0 ? "keine Server-/Deploy-Details in der Ausgabe" : `gefunden: ${leaks.join(", ")}`);

  // Die Seite muss zurück zum Download führen — sie ersetzt ihn nicht.
  const start = html.get("index.html");
  pruefe(start.includes('href="../"'), "Startseite verlinkt zurück auf die Download-Seite");
  pruefe(
    [...html.values()].every((t) => t.includes('href="../"')),
    "jede Seite trägt den Weg zum Programm-Download im Kopf"
  );
}

console.log(fehler === 0 ? "\nOK" : `\n${fehler} fehlgeschlagen`);
process.exit(fehler === 0 ? 0 : 1);
