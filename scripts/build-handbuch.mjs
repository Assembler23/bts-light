#!/usr/bin/env node
// Baut das öffentliche Handbuch für badhub.de/download/bts-light/handbuch/
// aus den Markdown-Dateien, die docs/handbuch.json auflistet.
//
//   node scripts/build-handbuch.mjs \
//     --manifest docs/handbuch.json \
//     --out dist/handbuch \
//     --basis .                       (Wurzel, aus der die Dateien gelesen werden)
//
// WARUM EIN MANIFEST STATT "alles unter docs/": Das Repo ist öffentlich, aber
// "im Repo lesbar" und "auf badhub.de als Anleitung veröffentlicht" sind zwei
// verschiedene Dinge. ADRs, Specs, Roadmaps und die Server-Einrichtung des
// Relays gehören nicht in ein Handbuch. Eine Whitelist kann nur zu wenig
// veröffentlichen (fällt beim Lesen auf), eine Blacklist zu viel (fällt
// niemandem auf) — deshalb Whitelist.
//
// WARUM marked: Die Handbuch-Seiten brauchen Überschriften, verschachtelte
// Listen, GFM-Tabellen, Codeblöcke und Querlinks. Der Release-Seiten-Generator
// (scripts/build-release-page.mjs) kann bewusst nur ein Mini-Subset aus
// Changelog-Bullets und taugt dafür nicht. marked ist eine reine
// devDependency mit NULL transitiven Paketen (Audit 05.09.2026), läuft nur
// hier in Node/CI und wird NICHT in die App gebündelt — der Zero-Dependency-
// Charakter der ausgelieferten Software bleibt unberührt.

import { marked } from "marked";
import { readFileSync, writeFileSync, mkdirSync, rmSync } from "node:fs";
import { join, dirname, basename } from "node:path";

function arg(name, fallback = null) {
  const i = process.argv.indexOf(`--${name}`);
  const v = i >= 0 ? process.argv[i + 1] : undefined;
  return v && !v.startsWith("--") ? v : fallback;
}

const manifestPath = arg("manifest", "docs/handbuch.json");
const outDir = arg("out", "dist/handbuch");
const basis = arg("basis", ".");

const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));

// Gesammelte Beanstandungen. Nichts davon bricht sofort ab — der Lauf soll
// ALLE Probleme auf einmal zeigen, nicht eins pro Durchlauf.
const probleme = [];

// ── Seitenliste flach ─────────────────────────────────────────────────────
const seiten = [];
for (const gruppe of manifest.gruppen) {
  for (const s of gruppe.seiten) seiten.push({ ...s, gruppe: gruppe.name });
}
// Nur echte Markdown-Seiten haben eine eigene HTML-Datei; "extern" ist ein
// Verweis nach draußen (z. B. zurück auf die Download-Seite).
const inhalt = seiten.filter((s) => s.datei);
const veroeffentlicht = new Map(inhalt.map((s) => [s.datei, s]));

// ── Hilfen ────────────────────────────────────────────────────────────────
function esc(s) {
  return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/**
 * Anker aus einem Überschriftentext — bewusst nach GitHub-Regel: kleinschreiben,
 * Satzzeichen weg, Leerzeichen zu Bindestrichen, Umlaute BLEIBEN. Damit
 * funktionieren die vorhandenen `#anker`-Querverweise in der Doku unverändert
 * weiter; eine eigene Regel (etwa ue statt ü) würde sie alle brechen.
 */
function anker(text) {
  return text
    .toLowerCase()
    .replace(/<[^>]+>/g, "")
    .replace(/[^\p{Letter}\p{Number}\s-]/gu, "")
    .trim()
    .replace(/\s+/g, "-");
}

/** HTML-Tags aus einem gerenderten Überschriftentext entfernen (für TOC/Anker). */
function nurText(html) {
  return html.replace(/<[^>]+>/g, "").replace(/&amp;/g, "&").replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">").replace(/&quot;/g, '"').trim();
}

/**
 * Abschnitte entfernen, die nicht ins Handbuch gehören.
 *
 * Zwei Wege, absichtlich beide:
 *  - "aus" im Manifest nennt Überschriften; der Abschnitt fliegt samt Inhalt
 *    bis zur nächsten gleich- oder höherrangigen Überschrift raus. Das hält
 *    halb-interne Dateien (Bedienung UND Architektur in einem Dokument)
 *    unzerschnitten und die Entscheidung an einem Ort sichtbar.
 *  - <!-- handbuch:aus --> … <!-- handbuch:an --> im Markdown selbst, für
 *    Stellen mitten in einem Abschnitt.
 *
 * Nicht gefundene "aus"-Titel sind ein FEHLER, kein stilles Nichts: Sonst
 * macht eine umbenannte Überschrift einen internen Abschnitt wieder öffentlich,
 * ohne dass es jemand merkt.
 */
function setextZuAtx(md) {
  // "Titel\n====" (H1) und "Titel\n----" (H2) sind nach CommonMark
  // Überschriften, und genau so rendert marked sie auch. Die
  // Abschnittserkennung unten sieht aber nur "#"-Zeilen. Ohne diese
  // Umschreibung endet ein Ausschluss an so einer Überschrift NICHT — er
  // frisst weiter bis zur nächsten "#"-Zeile und nimmt sichtbare Kapitel mit.
  // Das wäre stiller Textverlust: kein Leak, aber auch kein roter Test.
  const zeilen = md.split("\n");
  const raus = [];
  let imCode = false;
  for (let i = 0; i < zeilen.length; i++) {
    const z = zeilen[i];
    if (/^\s*```/.test(z)) imCode = !imCode;
    const naechste = zeilen[i + 1];
    const unterstrich = !imCode && naechste && /^(={2,}|-{2,})\s*$/.test(naechste);
    // Der Text darüber muss echter Absatz sein — keine Liste, kein Zitat,
    // keine Tabellenzeile (| --- |), keine bereits vorhandene Überschrift und
    // keine Leerzeile (dann ist "---" eine Trennlinie oder Frontmatter).
    const traegt = z.trim() !== "" && !/^\s*([#>|*+]|-\s|\d+\.\s)/.test(z);
    if (unterstrich && traegt) {
      raus.push(`${naechste.trim().startsWith("=") ? "#" : "##"} ${z.trim()}`);
      i++; // Unterstrich-Zeile überspringen
      continue;
    }
    raus.push(z);
  }
  return raus.join("\n");
}

function schneiden(md, ausTitel, quelle) {
  md = setextZuAtx(md);

  // 1) Marker-Bereiche
  let offen = 0;
  const behalten = [];
  for (const zeile of md.split("\n")) {
    if (/^\s*<!--\s*handbuch:aus\s*-->/.test(zeile)) { offen++; continue; }
    if (/^\s*<!--\s*handbuch:an\s*-->/.test(zeile)) {
      if (offen === 0) probleme.push(`${quelle}: 'handbuch:an' ohne offenes 'handbuch:aus'.`);
      else offen--;
      continue;
    }
    if (offen === 0) behalten.push(zeile);
  }
  if (offen > 0) probleme.push(`${quelle}: ${offen}× 'handbuch:aus' nie geschlossen.`);

  if (!ausTitel || ausTitel.length === 0) return behalten.join("\n");

  // 2) Benannte Abschnitte. Codeblöcke ausklammern — eine "## "-Zeile in einem
  //    ``` -Block ist keine Überschrift.
  const gefunden = new Set();
  const raus = [];
  let imCode = false;
  let entferneBis = 0; // 0 = nichts aktiv, sonst: Ebene des entfernten Abschnitts
  for (const zeile of behalten) {
    if (/^\s*```/.test(zeile)) imCode = !imCode;
    // {1,6}: Auch eine H1 mitten im Dokument beendet einen Ausschluss.
    const h = imCode ? null : zeile.match(/^(#{1,6})\s+(.*?)\s*$/);
    if (h) {
      const ebene = h[1].length;
      const titel = h[2].trim();
      if (entferneBis && ebene <= entferneBis) entferneBis = 0;
      if (!entferneBis && ausTitel.some((t) => t.trim() === titel)) {
        gefunden.add(titel);
        entferneBis = ebene;
      }
    }
    if (!entferneBis) raus.push(zeile);
  }
  for (const t of ausTitel) {
    if (!gefunden.has(t.trim())) {
      probleme.push(`${quelle}: Abschnitt "${t}" aus 'aus' existiert nicht (umbenannt oder gelöscht?) — er wäre sonst wieder öffentlich.`);
    }
  }
  return raus.join("\n");
}

/**
 * Querverweise umschreiben.
 *  - Link auf eine veröffentlichte Datei  → deren <slug>.html (Anker bleibt)
 *  - Link auf eine NICHT veröffentlichte  → Link wird entwertet, der Text bleibt
 *  - #anker, http(s):, mailto:            → unverändert
 *
 * Auf 404 zeigen lassen ist keine Option: build-release-page.mjs entfernt
 * relative Doku-Links aus demselben Grund ganz (dort laufen sie ins Leere).
 */
function linksUmschreiben(html, quelle) {
  return html.replace(/<a href="([^"]*)"([^>]*)>([\s\S]*?)<\/a>/g, (all, href, rest, text) => {
    if (/^(https?:|mailto:|#)/i.test(href)) return all;
    const [pfad, ankerTeil] = href.split("#");
    if (!pfad) return all;
    // Pfad relativ zur Quelldatei auflösen, dann auf Repo-Wurzel normalisieren.
    const roh = pfad.startsWith("docs/") || pfad.startsWith("./docs/")
      ? pfad.replace(/^\.\//, "")
      : join(dirname(quelle), pfad).replace(/\\/g, "/");
    const ziel = veroeffentlicht.get(roh);
    if (ziel) return `<a href="${ziel.slug}.html${ankerTeil ? "#" + ankerTeil : ""}"${rest}>${text}</a>`;
    return text;
  });
}

// ── Eine Seite rendern ────────────────────────────────────────────────────
function seiteBauen(s) {
  const md = readFileSync(join(basis, s.datei), "utf8");
  const geschnitten = schneiden(md, s.aus, s.datei);

  // Die H1 der Quelldatei fliegt raus — die Überschrift der Seite kommt aus
  // dem Manifest, damit Navigation und Seitentitel garantiert übereinstimmen.
  const ohneH1 = geschnitten.replace(/^#\s+.*$/m, "");

  let html = marked.parse(ohneH1, { gfm: true, mangle: false });

  // Anker + Inhaltsverzeichnis aus den H2. IDs erst hier vergeben: marked
  // setzt seit v16 keine mehr, und wir brauchen die GitHub-Regel (s. anker()).
  const toc = [];
  const belegt = new Set();
  html = html.replace(/<(h[2-4])>([\s\S]*?)<\/\1>/g, (all, tag, innen) => {
    const text = nurText(innen);
    let id = anker(text) || "abschnitt";
    // Gleichnamige Überschriften in einer Datei kommen vor (etwa "Grenzen").
    let n = 2;
    while (belegt.has(id)) id = `${anker(text)}-${n++}`;
    belegt.add(id);
    if (tag === "h2") toc.push({ id, text });
    return `<${tag} id="${id}">${innen}<a class="ankerlink" href="#${id}" aria-label="Link zu diesem Abschnitt">#</a></${tag}>`;
  });

  html = linksUmschreiben(html, s.datei);

  // Breite Tabellen dürfen die Seite nicht seitlich schieben — sie scrollen
  // in ihrem eigenen Kasten. Auf dem Handy sonst unlesbar.
  html = html.replace(/<table>/g, '<div class="tabelle"><table>').replace(/<\/table>/g, "</table></div>");

  return { html, toc };
}

// ── Seitengerüst ──────────────────────────────────────────────────────────
const CSS = `
  :root { color-scheme: light; }
  * { box-sizing: border-box; }
  body { margin: 0; font-family: system-ui, -apple-system, "Segoe UI", sans-serif;
         background: #f4f6f8; color: #1a202c; line-height: 1.6; }
  header { background: #0f2740; color: #fff; padding: 1.4rem 1.2rem; }
  header .wrap { max-width: 1180px; margin: 0 auto; display: flex; align-items: baseline;
                 justify-content: space-between; gap: 1rem; flex-wrap: wrap; }
  header a.marke { color: #fff; text-decoration: none; font-size: 1.25rem; font-weight: 700; }
  header .sub { opacity: .8; font-size: .9rem; display: block; font-weight: 400; margin-top: .2rem; }
  header a.dl { background: #2f855a; color: #fff; padding: .45rem 1rem; border-radius: 8px;
                text-decoration: none; font-weight: 600; white-space: nowrap; }
  .layout { max-width: 1180px; margin: 0 auto; display: flex; gap: 2rem; padding: 1.6rem 1.2rem 3rem;
            align-items: flex-start; }
  nav.kapitel { flex: 0 0 250px; position: sticky; top: 1rem; font-size: .9rem; }
  nav.kapitel h2 { font-size: .72rem; text-transform: uppercase; letter-spacing: .06em;
                   color: #718096; margin: 1.1rem 0 .35rem; }
  nav.kapitel h2:first-child { margin-top: 0; }
  nav.kapitel a { display: block; padding: .28rem .55rem; border-radius: 6px; color: #2d3748;
                  text-decoration: none; }
  nav.kapitel a:hover { background: #e2e8f0; }
  nav.kapitel a.hier { background: #0f2740; color: #fff; font-weight: 600; }
  main { flex: 1 1 auto; min-width: 0; background: #fff; border: 1px solid #e2e8f0;
         border-radius: 10px; padding: 1.6rem 2rem 2.4rem; }
  main h1 { margin: 0 0 1.2rem; font-size: 1.7rem; color: #0f2740; }
  main h2 { margin: 2rem 0 .6rem; font-size: 1.25rem; color: #0f2740;
            border-bottom: 1px solid #e2e8f0; padding-bottom: .3rem; }
  main h3 { margin: 1.5rem 0 .4rem; font-size: 1.05rem; }
  main h4 { margin: 1.2rem 0 .3rem; font-size: .95rem; }
  .ankerlink { color: #cbd5e0; text-decoration: none; margin-left: .4rem; font-weight: 400;
               opacity: 0; }
  h2:hover .ankerlink, h3:hover .ankerlink, h4:hover .ankerlink { opacity: 1; }
  main a { color: #2b6cb0; }
  code { background: #edf2f7; padding: .05rem .3rem; border-radius: 4px; font-size: .9em;
         word-break: break-word; }
  pre { background: #0f2740; color: #e2e8f0; padding: .9rem 1.1rem; border-radius: 8px;
        overflow-x: auto; }
  pre code { background: none; color: inherit; padding: 0; }
  blockquote { margin: 1rem 0; padding: .1rem 1rem; border-left: 4px solid #2f855a;
               background: #f7fafc; color: #2d3748; }
  .tabelle { overflow-x: auto; margin: 1rem 0; }
  table { border-collapse: collapse; width: 100%; font-size: .92rem; }
  th, td { border: 1px solid #e2e8f0; padding: .4rem .6rem; text-align: left; vertical-align: top; }
  th { background: #edf2f7; }
  img { max-width: 100%; height: auto; border-radius: 8px; }
  .toc { background: #f7fafc; border: 1px solid #e2e8f0; border-radius: 8px;
         padding: .8rem 1rem; margin-bottom: 1.6rem; font-size: .9rem; }
  .toc strong { display: block; margin-bottom: .3rem; color: #4a5568; font-size: .78rem;
                text-transform: uppercase; letter-spacing: .05em; }
  .toc ul { margin: 0; padding-left: 1.1rem; columns: 2; column-gap: 1.6rem; }
  .toc li { margin: .12rem 0; break-inside: avoid; }
  .blaettern { display: flex; justify-content: space-between; gap: 1rem; margin-top: 2.4rem;
               padding-top: 1.2rem; border-top: 1px solid #e2e8f0; font-size: .9rem; }
  .blaettern a { display: block; max-width: 46%; text-decoration: none; color: #2b6cb0; }
  .blaettern span { display: block; color: #a0aec0; font-size: .78rem; }
  .gruppe { border: 1px solid #e2e8f0; border-radius: 10px; padding: .9rem 1.2rem; margin-bottom: 1rem;
            background: #fdfdfe; }
  .gruppe h2 { margin: 0 0 .4rem; font-size: 1.05rem; border: 0; padding: 0; }
  .gruppe ul { margin: 0; padding-left: 1.1rem; }
  .gruppe li { margin: .25rem 0; }
  footer { text-align: center; color: #718096; font-size: .8rem; padding: 0 1rem 2.5rem; }
  @media (max-width: 860px) {
    .layout { flex-direction: column; padding-top: 1rem; }
    nav.kapitel { position: static; flex: 1 1 auto; width: 100%; background: #fff;
                  border: 1px solid #e2e8f0; border-radius: 10px; padding: .9rem 1rem; }
    main { padding: 1.2rem 1.1rem 2rem; }
    .toc ul { columns: 1; }
  }
`;

function navHtml(aktiv) {
  return manifest.gruppen
    .map((g) => {
      const eintraege = g.seiten
        .map((s) => {
          const href = s.datei ? `${s.slug}.html` : s.extern;
          const hier = s.datei && s.slug === aktiv ? ' class="hier"' : "";
          return `      <a href="${esc(href)}"${hier}>${esc(s.titel)}</a>`;
        })
        .join("\n");
      return `    <h2>${esc(g.name)}</h2>\n${eintraege}`;
    })
    .join("\n");
}

const stand = new Date().toISOString().slice(0, 10);

function rahmen({ slug, titel, inhaltHtml }) {
  return `<!DOCTYPE html>
<html lang="de">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${esc(titel)} – ${esc(manifest.titel)}</title>
<style>${CSS}</style>
</head>
<body>
<header>
  <div class="wrap">
    <a class="marke" href="index.html">${esc(manifest.titel)}
      <span class="sub">${esc(manifest.untertitel)}</span></a>
    <a class="dl" href="../">Programm herunterladen</a>
  </div>
</header>
<div class="layout">
  <nav class="kapitel">
${navHtml(slug)}
  </nav>
  <main>
${inhaltHtml}
  </main>
</div>
<footer>Erzeugt aus der Projektdokumentation · Stand ${stand} · badhub.de</footer>
</body>
</html>
`;
}

// ── Startseite ────────────────────────────────────────────────────────────
function startseite() {
  const gruppen = manifest.gruppen
    .map((g) => {
      const li = g.seiten
        .map((s) => {
          const href = s.datei ? `${s.slug}.html` : s.extern;
          const be = s.beschreibung ? ` — ${esc(s.beschreibung)}` : "";
          return `      <li><a href="${esc(href)}">${esc(s.titel)}</a>${be}</li>`;
        })
        .join("\n");
      return `  <section class="gruppe">\n    <h2>${esc(g.name)}</h2>\n    <ul>\n${li}\n    </ul>\n  </section>`;
    })
    .join("\n");
  return rahmen({
    slug: "index",
    titel: "Übersicht",
    inhaltHtml: `<h1>${esc(manifest.titel)}</h1>\n<p>${esc(manifest.untertitel)}</p>\n${gruppen}`,
  });
}

// ── Bauen ─────────────────────────────────────────────────────────────────
const gebaut = [];
for (let i = 0; i < inhalt.length; i++) {
  const s = inhalt[i];
  const { html, toc } = seiteBauen(s);
  const tocHtml =
    toc.length >= 3
      ? `<div class="toc"><strong>Auf dieser Seite</strong><ul>` +
        toc.map((t) => `<li><a href="#${t.id}">${esc(t.text)}</a></li>`).join("") +
        `</ul></div>`
      : "";
  const vor = inhalt[i - 1];
  const nach = inhalt[i + 1];
  const blaettern =
    `<div class="blaettern">` +
    (vor ? `<a href="${vor.slug}.html"><span>Zurück</span>${esc(vor.titel)}</a>` : "<span></span>") +
    (nach ? `<a href="${nach.slug}.html" style="text-align:right"><span>Weiter</span>${esc(nach.titel)}</a>` : "<span></span>") +
    `</div>`;
  gebaut.push({
    name: `${s.slug}.html`,
    text: rahmen({
      slug: s.slug,
      titel: s.titel,
      inhaltHtml: `<h1>${esc(s.titel)}</h1>\n${tocHtml}\n${html}\n${blaettern}`,
    }),
  });
}
gebaut.push({ name: "index.html", text: startseite() });

// ── Ergebnis ──────────────────────────────────────────────────────────────
if (probleme.length) {
  for (const p of probleme) console.error(`FEHLER: ${p}`);
  console.error(`\n${probleme.length} Problem(e) — nichts geschrieben.`);
  process.exit(1);
}
// --delete-Ersatz: der Zielordner wird IMMER frisch gebaut, damit eine aus dem
// Manifest entfernte Seite nicht als Karteileiche liegen bleibt.
rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });
for (const d of gebaut) writeFileSync(join(outDir, d.name), d.text);
console.error(`${outDir}: ${gebaut.length} Seiten geschrieben (${inhalt.length} Kapitel + Startseite).`);
