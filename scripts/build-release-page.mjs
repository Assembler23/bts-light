#!/usr/bin/env node
// Baut die öffentliche Release-Seite für badhub.de/download/bts-light/
// aus docs/changelog.md. Läuft im Release-Workflow (publish-Job) und
// lokal ohne Abhängigkeiten (Node ≥ 18).
//
//   node scripts/build-release-page.mjs \
//     --changelog docs/changelog.md \
//     --files vorhandene-exes.txt \      (eine Datei je Zeile; optional)
//     --out index.html \
//     --notes-out notes.txt --notes-version 0.9.147   (optional)
//     --dates dates.txt                               (optional)
//
// --dates: Datei „<version> <YYYY-MM-DD>" je Zeile (Rest ignoriert) — die
// Seite zeigt je Version das Datum (TT.MM.JJJJ). Quelle im Release-Workflow:
// `git for-each-ref --format '%(refname:short) %(creatordate:short)' refs/tags`.
//
// --files: nur Versionen, deren Installer wirklich auf dem Server liegt,
// bekommen einen Download-Knopf (alte/TEST-Versionen fehlen teils).
// --notes-out: schreibt die Stichpunkte EINER Version als Klartext —
// der Workflow hängt sie an latest.json (`notes`), damit das
// Update-Fenster in der App „Was ist neu" zeigt.

import { readFileSync, writeFileSync } from "node:fs";

function arg(name, fallback = null) {
  const i = process.argv.indexOf(`--${name}`);
  const v = i >= 0 ? process.argv[i + 1] : undefined;
  // Folgt direkt das nächste Flag, wurde der Wert vergessen → Fallback.
  return v && !v.startsWith("--") ? v : fallback;
}

const changelogPath = arg("changelog", "docs/changelog.md");
const filesPath = arg("files");
const outPath = arg("out", "index.html");
const notesOut = arg("notes-out");
const notesVersion = arg("notes-version");
// Optionale Datei „Version Datum" je Zeile (z. B. aus `git for-each-ref`
// über die Tags). Fehlt sie, bleibt die Datumsangabe je Version leer.
const datesPath = arg("dates");

const md = readFileSync(changelogPath, "utf8");

// ── Changelog parsen: "## vX.Y.Z"-Abschnitte mit Stichpunkten ─────────────
const sections = [];
let current = null;
for (const line of md.split("\n")) {
  const h = line.match(/^## v(\d+\.\d+\.\d+)\s*$/);
  if (h) {
    current = { version: h[1], lines: [] };
    sections.push(current);
    continue;
  }
  if (current) current.lines.push(line);
}
if (sections.length === 0) {
  console.error("Kein '## vX.Y.Z'-Abschnitt im Changelog gefunden.");
  process.exit(1);
}

// Stichpunkte je Version: Markdown-Bullets zusammenfassen (Folgezeilen
// eines Bullets werden angehängt), Nicht-Bullet-Prosa ignoriert.
function bullets(lines) {
  const out = [];
  for (const raw of lines) {
    if (/^- /.test(raw)) out.push(raw.slice(2).trim());
    else if (/^\s+\S/.test(raw) && out.length) out[out.length - 1] += " " + raw.trim();
  }
  return out;
}

// Markdown-Reste für die Anzeige aufbereiten: **fett** → <strong>,
// `code` → <code>, [Text](link) → nur Text (relative Doku-Links laufen
// auf der Release-Seite ins Leere), ~~…~~ → durchgestrichen.
function esc(s) {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
function inlineHtml(s) {
  return esc(s)
    .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>")
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/~~(.+?)~~/g, "<s>$1</s>")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1");
}
function plainText(s) {
  return s
    .replace(/\*\*(.+?)\*\*/g, "$1")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/~~(.+?)~~/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1");
}

// ── Release-Daten je Version (optional) ───────────────────────────────────
// Zeilenformat „<version> <YYYY-MM-DD>" (weitere Spalten werden ignoriert),
// z. B. direkt aus `git for-each-ref --format '%(refname:short) %(creatordate:short)'`
// mit vorangestelltem „v"-Strip. Ausgabe: deutsches Datum TT.MM.JJJJ.
const dates = new Map();
if (datesPath) {
  for (const line of readFileSync(datesPath, "utf8").split("\n")) {
    const m = line.trim().match(/^v?(\d+\.\d+\.\d+)\s+(\d{4})-(\d{2})-(\d{2})/);
    if (m) dates.set(m[1], `${m[4]}.${m[3]}.${m[2]}`);
  }
}
function dateOf(version) {
  return dates.get(version) || "";
}

// ── Vorhandene Installer (optional) ───────────────────────────────────────
let available = null;
if (filesPath) {
  available = new Set(
    readFileSync(filesPath, "utf8")
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean)
  );
}
function setupName(version) {
  return `BTS.Light_${version}_x64-setup.exe`;
}
function hasInstaller(version) {
  return available ? available.has(setupName(version)) : true;
}

// ── notes.txt für latest.json (eine Version, Klartext) ────────────────────
if (notesOut && notesVersion) {
  const sec = sections.find((s) => s.version === notesVersion);
  if (!sec) {
    // Sichtbar warnen: die Version fehlt im Changelog → das Update-Fenster
    // bekäme nur einen generischen Einzeiler (release.md: Abschnitt VOR
    // dem Taggen anlegen!). Kein Abbruch — der Release selbst ist gültig.
    console.error(
      `WARNUNG: docs/changelog.md hat keinen Abschnitt '## v${notesVersion}' — notes bleiben generisch.`
    );
  }
  const text = sec
    ? bullets(sec.lines)
        .map((b) => "• " + plainText(b))
        .join("\n")
    : `BTS Light ${notesVersion}`;
  writeFileSync(notesOut, text);
  console.error(`notes.txt für v${notesVersion} geschrieben (${text.length} Zeichen).`);
}

// ── Seite rendern ─────────────────────────────────────────────────────────
const latest = sections[0];
const generated = new Date().toISOString().slice(0, 10);

const versionHtml = sections
  .map((sec, i) => {
    const items = bullets(sec.lines)
      .map((b) => `        <li>${inlineHtml(b)}</li>`)
      .join("\n");
    const dl = hasInstaller(sec.version)
      ? `<a class="dl${i === 0 ? " primary" : ""}" href="${setupName(sec.version)}">Download</a>`
      : `<span class="nodl">kein Installer verfügbar</span>`;
    const date = dateOf(sec.version);
    const dateHtml = date ? ` <span class="vdate">${date}</span>` : "";
    return `
    <section class="version${i === 0 ? " latest" : ""}" id="v${sec.version}">
      <div class="vhead">
        <h2>Version ${sec.version}${dateHtml}${i === 0 ? ' <span class="badge">aktuell</span>' : ""}</h2>
        ${dl}
      </div>
      <ul>
${items}
      </ul>
    </section>`;
  })
  .join("\n");

const html = `<!DOCTYPE html>
<html lang="de">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>BTS Light – Downloads &amp; Versionen</title>
<style>
  :root { color-scheme: light; }
  * { box-sizing: border-box; }
  body { margin: 0; font-family: system-ui, -apple-system, "Segoe UI", sans-serif;
         background: #f4f6f8; color: #1a202c; line-height: 1.55; }
  header { background: #0f2740; color: #fff; padding: 2.2rem 1.2rem; }
  header .wrap, main { max-width: 860px; margin: 0 auto; }
  header h1 { margin: 0 0 .3rem; font-size: 1.7rem; }
  header p { margin: 0; opacity: .85; }
  header .stable { display: inline-block; margin-top: 1rem; background: #2f855a; color: #fff;
                   padding: .55rem 1.1rem; border-radius: 8px; text-decoration: none; font-weight: 600; }
  main { padding: 1.4rem 1.2rem 3rem; }
  .version { background: #fff; border: 1px solid #e2e8f0; border-radius: 10px;
             padding: 1rem 1.2rem; margin-bottom: 1rem; }
  .version.latest { border-color: #2f855a; box-shadow: 0 1px 6px rgba(47,133,90,.18); }
  .vhead { display: flex; align-items: center; justify-content: space-between; gap: .8rem; flex-wrap: wrap; }
  .vhead h2 { margin: 0; font-size: 1.15rem; }
  .badge { background: #2f855a; color: #fff; font-size: .7rem; padding: .15rem .5rem;
           border-radius: 999px; vertical-align: middle; }
  .vdate { color: #718096; font-size: .8rem; font-weight: 400; margin-left: .1rem; }
  a.dl { background: #edf2f7; color: #1a202c; border: 1px solid #cbd5e0; padding: .35rem .9rem;
         border-radius: 7px; text-decoration: none; font-weight: 600; white-space: nowrap; }
  a.dl.primary { background: #2f855a; border-color: #2f855a; color: #fff; }
  .nodl { color: #a0aec0; font-size: .85rem; }
  ul { margin: .7rem 0 0; padding-left: 1.2rem; }
  li { margin-bottom: .45rem; }
  li strong { color: #0f2740; }
  code { background: #edf2f7; padding: 0 .3rem; border-radius: 4px; font-size: .9em; }
  footer { text-align: center; color: #718096; font-size: .8rem; padding: 0 1rem 2rem; }
</style>
</head>
<body>
<header>
  <div class="wrap">
    <h1>BTS Light</h1>
    <p>Plug-and-play-Brücke zwischen BTP (Badminton Tournament Planner) und dem badhub.de-Liveticker – mit Tablet-Spielzettel und Court-Monitoren.</p>
    <a class="stable" href="BTS.Light-setup.exe">Aktuelle Version herunterladen (v${latest.version})</a>
  </div>
</header>
<main>
${versionHtml}
</main>
<footer>Automatisch erzeugt aus dem Änderungsverlauf · Stand ${generated} · badhub.de</footer>
</body>
</html>
`;

writeFileSync(outPath, html);
console.error(`${outPath} geschrieben: ${sections.length} Versionen, aktuell v${latest.version}.`);
