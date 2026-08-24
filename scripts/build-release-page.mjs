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
//     --notes-since 0.9.140                           (optional)
//     --dates dates.txt                               (optional)
//
// --dates: Datei „<version> <YYYY-MM-DD>" je Zeile (Rest ignoriert) — die
// Seite zeigt je Version das Datum (TT.MM.JJJJ). Quelle im Release-Workflow:
// `git for-each-ref --format '%(refname:short) %(creatordate:short)' refs/tags`.
//
// --files: nur Versionen, deren Installer wirklich auf dem Server liegt,
// bekommen einen Download-Knopf (alte/TEST-Versionen fehlen teils).
// --notes-out: schreibt die Stichpunkte als Klartext — der Workflow hängt
// sie an latest.json (`notes`), damit das Update-Fenster in der App
// „Was ist neu" zeigt.
//
// --notes-since: Version des ZULETZT veröffentlichten Tags. Damit umfassen
// die Notes ALLE Versionen dazwischen, nicht nur die getaggte. Das ist der
// Normalfall, nicht die Ausnahme: Zwischen zwei Tags liegen regelmäßig
// mehrere Versionssprünge (v0.9.214 → v0.9.223 waren neun), und wer
// aktualisiert, springt genau über diese Strecke. Ohne die Angabe sah er im
// Update-Fenster nur den letzten Eintrag und hielt acht Änderungen für
// nicht vorhanden.

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
const notesSince = arg("notes-since");
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

// ── notes.txt für latest.json (Klartext) ──────────────────────────────────
//
// Umfang: alle Versionen seit dem zuletzt veröffentlichten Tag (--notes-since,
// ausschliesslich) bis zur getaggten (--notes-version, einschliesslich). Ohne
// --notes-since bleibt es bei der einen getaggten Version.
//
// LÄNGE: Das Update-Fenster ist ein Dialog, kein Dokument. Passen die
// vollständigen Stichpunkte nicht in NOTES_MAX, wird auf je eine Kopfzeile
// pro Version gekürzt (die fett ausgezeichnete Kernaussage) und auf die
// Release-Seite verwiesen — lieber neun lesbare Zeilen als eine Textwand,
// die niemand liest.
const NOTES_MAX = 4000;

/**
 * "a.b.c" → [a, b, c]; alles andere → null.
 *
 * Bewusst streng: Ein stilles NaN→0 wuerde einen kaputten Wert (etwa einen
 * Tag-Namen ohne Versionsform) in eine gueltig aussehende 0.0.0 verwandeln,
 * und --notes-since waere wirkungslos, ohne dass jemand es merkt.
 */
function parseVersion(v) {
  const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(String(v).trim());
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}

/** Versionen vergleichen: <0, 0, >0 wie bei sort(). Nur fuer geprueftes Format. */
function cmpVersion(a, b) {
  const pa = parseVersion(a);
  const pb = parseVersion(b);
  if (!pa || !pb) throw new Error(`Unbrauchbare Version im Vergleich: '${a}' / '${b}'`);
  for (let i = 0; i < 3; i++) if (pa[i] !== pb[i]) return pa[i] - pb[i];
  return 0;
}

// Laengste Zeile der Kurzfassung. Haerte, keine Schaetzung: Ohne die Kappung
// haengt die Einhaltung von NOTES_MAX allein daran, wie knapp jemand seine
// Changelog-Kopfzeilen formuliert.
const KOPFZEILE_MAX = 90;

/** Kernaussage eines Stichpunkts: der erste **fette** Teil, sonst Satz 1. */
function schlagzeile(bullet) {
  const fett = bullet.match(/\*\*(.+?)\*\*/s);
  const roh = fett ? fett[1] : bullet.split(/(?<=\.)\s/)[0];
  const text = plainText(roh || "").replace(/\s+/g, " ").trim();
  if (text === "") return "Änderungen";
  return text.length > KOPFZEILE_MAX ? text.slice(0, KOPFZEILE_MAX - 1).trimEnd() + "…" : text;
}

if (notesOut && notesVersion) {
  // Bereich bestimmen: neueste zuerst, damit oben steht, was gerade kommt.
  // --notes-since wird geprueft, bevor es wirkt. Jeder Zweifelsfall faellt
  // auf "nur die getaggte Version" zurueck — der Fehler geht damit in
  // Richtung "zu wenig", nie in Richtung "kompletter Changelog seit v0.4.0".
  //
  // Drei Faelle fuehren zum Rueckfall:
  //  - kein Flag (erster Tag ueberhaupt, flacher Checkout),
  //  - unbrauchbarer Wert (git describe hat einen Nicht-Versions-Tag
  //    geliefert — deshalb steht im Workflow --match 'v[0-9]*'),
  //  - since >= version (Tag auf demselben Commit, Tag ausserhalb der
  //    Ahnenlinie). Ohne diesen Zweig bliebe der Bereich LEER und die
  //    Notes der getaggten Version gingen verloren.
  let seit = null;
  if (notesSince) {
    if (!parseVersion(notesSince)) {
      console.error(`WARNUNG: --notes-since '${notesSince}' ist keine Version — ignoriert.`);
    } else if (!parseVersion(notesVersion)) {
      console.error(`WARNUNG: --notes-version '${notesVersion}' ist keine Version — Bereich ignoriert.`);
    } else if (cmpVersion(notesSince, notesVersion) >= 0) {
      console.error(
        `WARNUNG: --notes-since ${notesSince} liegt nicht vor ${notesVersion} — ignoriert.`
      );
    } else {
      seit = notesSince;
    }
  }

  const imBereich = sections
    .filter((s) =>
      seit
        ? cmpVersion(s.version, notesVersion) <= 0 && cmpVersion(s.version, seit) > 0
        : s.version === notesVersion
    )
    .sort((a, b) => cmpVersion(b.version, a.version));

  if (!imBereich.some((s) => s.version === notesVersion)) {
    // Sichtbar warnen: die getaggte Version fehlt im Changelog → das
    // Update-Fenster bekäme nur einen generischen Einzeiler (docs/release.md:
    // Abschnitt VOR dem Taggen anlegen!). Kein Abbruch — der Release gilt.
    console.error(
      `WARNUNG: docs/changelog.md hat keinen Abschnitt '## v${notesVersion}' — notes bleiben generisch.`
    );
  }

  let text;
  if (imBereich.length === 0) {
    text = `BTS Light ${notesVersion}`;
  } else if (imBereich.length === 1) {
    // Einzelne Version: unverändert wie bisher, ohne Kopfzeile.
    text = bullets(imBereich[0].lines)
      .map((b) => "• " + plainText(b))
      .join("\n");
  } else {
    const kopf = `Dieses Update fasst ${imBereich.length} Versionen zusammen `
      + `(v${imBereich[imBereich.length - 1].version} – v${imBereich[0].version}):`;
    const voll = [kopf, ""];
    for (const sec of imBereich) {
      voll.push(`v${sec.version}`);
      for (const b of bullets(sec.lines)) voll.push("• " + plainText(b));
      voll.push("");
    }
    text = voll.join("\n").trimEnd();

    if (text.length > NOTES_MAX) {
      const kurz = [kopf, ""];
      for (const sec of imBereich) {
        const bs = bullets(sec.lines);
        kurz.push(`• v${sec.version}: ${bs.length ? schlagzeile(bs[0]) : "Änderungen"}`);
      }
      kurz.push("", "Alle Einzelheiten: badhub.de/download/bts-light/");
      text = kurz.join("\n");

      // Letzte Sicherung: Selbst bei sehr vielen Versionen bleibt der Dialog
      // endlich. Die Kopfzeilen sind zwar je Zeile gekappt, die ANZAHL ist
      // es nicht — bei 200 gebuendelten Versionen reisst auch die Kurzform.
      if (text.length > NOTES_MAX) {
        const verweis = "\n\nAlle Einzelheiten: badhub.de/download/bts-light/";
        text = text.slice(0, NOTES_MAX - verweis.length).trimEnd() + verweis;
      }
    }
  }

  writeFileSync(notesOut, text);
  const umfang =
    imBereich.length > 1 ? `v${imBereich[imBereich.length - 1].version}–v${notesVersion}` : `v${notesVersion}`;
  console.error(`notes.txt für ${umfang} geschrieben (${text.length} Zeichen, ${imBereich.length} Version(en)).`);
}

// ── Pi-Image (Court-Monitore) ──────────────────────────────────
// Die SD-Karten fuer die Court-Monitore werden vor Ort beschrieben — der
// Link stand bisher nur in docs/pi-dual-image.md im Repo, das ein Aufbau-Team
// in der Halle nicht hat. Er gehoert deshalb auf die oeffentliche Seite.
// Das Image liegt NICHT im Release-Workflow, sondern wird per rsync gepflegt
// (siehe docs/pi-dual-image.md) — die Datei ist hier bewusst fest verdrahtet
// und nicht aus --files abgeleitet.
const PI_IMAGE_URL = "pi-image/bts-light-pi.img.xz";
const PI_IMAGE_SHA_URL = "pi-image/bts-light-pi.img.xz.sha256";

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
  header .stable { display: inline-block; margin-top: 1rem; margin-right: .6rem; background: #2f855a;
                   color: #fff; padding: .55rem 1.1rem; border-radius: 8px; text-decoration: none;
                   font-weight: 600; }
  header .stable.ghost { background: transparent; border: 1px solid rgba(255,255,255,.55); }
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
  .pi { background: #fff; border: 1px solid #cbd5e0; border-left: 4px solid #0f2740;
        border-radius: 10px; padding: 1rem 1.2rem; margin-bottom: 1.4rem; }
  .pi h2 { margin: 0 0 .4rem; font-size: 1.05rem; }
  .pi p { margin: .4rem 0; }
  .pi ol { margin: .5rem 0 0; padding-left: 1.2rem; }
  .pi .sha { font-size: .8rem; color: #718096; }
  footer { text-align: center; color: #718096; font-size: .8rem; padding: 0 1rem 2rem; }
</style>
</head>
<body>
<header>
  <div class="wrap">
    <h1>BTS Light</h1>
    <p>Plug-and-play-Brücke zwischen BTP (Badminton Tournament Planner) und dem badhub.de-Liveticker – mit Tablet-Spielzettel und Court-Monitoren.</p>
    <a class="stable" href="BTS.Light-setup.exe">Aktuelle Version herunterladen (v${latest.version})</a>
    <a class="stable ghost" href="#pi-image">Pi-Image für Court-Monitore</a>
  </div>
</header>
<main>
  <section class="pi" id="pi-image">
    <div class="vhead">
      <h2>Court-Monitore: Raspberry-Pi-Image für die SD-Karten</h2>
      <a class="dl" href="${PI_IMAGE_URL}">Image herunterladen</a>
    </div>
    <p>Ein Image für beide Systeme — der Pi findet beim Start selbst, ob BTS oder
       BTS Light im Hallen-WLAN läuft. Rund 1&nbsp;GB gepackt, wächst beim ersten Boot
       auf die volle Kartengröße (jede Karte ab 4&nbsp;GB).</p>
    <ol>
      <li>Im <strong>Raspberry Pi Imager</strong> „Eigenes Image verwenden“ wählen und die
          <code>.img.xz</code> angeben — nicht vorher entpacken.</li>
      <li>Ziel-Karte wählen und schreiben.</li>
      <li><strong>Keine</strong> Imager-Anpassungen (Hostname, WLAN, SSH) setzen — WLAN und
          Kiosk-Start sind im Image enthalten und würden überschrieben.</li>
      <li>Karte in den Pi, einschalten — der Kiosk startet von allein.</li>
    </ol>
    <p class="sha">Prüfsumme: <a href="${PI_IMAGE_SHA_URL}">bts-light-pi.img.xz.sha256</a></p>
  </section>
${versionHtml}
</main>
<footer>Automatisch erzeugt aus dem Änderungsverlauf · Stand ${generated} · badhub.de</footer>
</body>
</html>
`;

writeFileSync(outPath, html);
console.error(`${outPath} geschrieben: ${sections.length} Versionen, aktuell v${latest.version}.`);
