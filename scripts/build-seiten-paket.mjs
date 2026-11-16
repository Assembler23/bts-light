// Baut das Seiten-Paket für „Hotfix ohne Neustart" (Spec
// docs/features/hotfix-ohne-neustart.md, ADR 0099).
//
// Ein Paket enthält ALLE Geräte-Seiten des Turnier-PCs und gilt für genau
// eine Kern-Version: die des Tags, von dem der Hotfix-Zweig abzweigt. Der
// Zweig darf deshalb nur Seiten ändern — sobald er einen Kern-Pfad anfasst,
// passen die Seiten nicht mehr sicher zur installierten Programmdatei, und
// der Bau bricht ab.
//
// VERTRAUEN: Dieses Skript läuft aus `main`. Den Hotfix-Zweig liest es nur
// als DATEN, und zwar ausschließlich über git (`git show`, `git ls-tree`) aus
// dem Repository unter `--repo` — nie aus dessen Arbeitsverzeichnis, nie als
// Code. So kann der Zweig weder das Prüfen noch das Bauen umbiegen, und ein
// Verweis (Symlink) unter den Seiten wird nie verfolgt.
//
// Aufrufe (Workflow .github/workflows/seiten-paket.yml):
//   plan     --repo <dir> --tag v1.2.3 --commit <sha>   prüft, gibt kern/folge/… aus
//   bericht  --repo <dir> --tag v1.2.3 --commit <sha>   Zusammenfassung für die Freigabe
//   nutzlast --repo <dir> --tag v1.2.3 --commit <sha> --out <datei>
//   index    --liste <datei mit Dateinamen> --out <datei>
// Nur für den Rust-Test (liest die Seiten von der Platte, ohne git):
//   nutzlast --aus-dateien <repo-wurzel> --kern … --folge … --commit … --out <datei>
//
// Signiert wird NICHT hier, sondern im Workflow nach der Freigabe.
import { execFileSync } from "node:child_process";
import { lstatSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

/** So viele Kern-Versionen nennt der Index höchstens. */
export const KERN_VERSIONEN_IM_INDEX = 3;

/** Einziges Paket-Format, das die App kennt. */
export const FORMAT = 1;

/** Obergrenze der Folge — hält Tippfehler und Unfug aus dem Index. */
export const MAX_FOLGE = 100000;

const ASSETS_RS = "src-tauri/src/tablet/assets.rs";
const SEITEN_ORDNER = "src-tauri/assets";
const SEITE_PFAD = /^src-tauri\/assets\/[^/]+\.html$/;

/**
 * Kern-Pfad = alles, was in die Programmdatei eingeht. Die Geräte-Seiten
 * selbst (`src-tauri/assets/*.html`) sind die eine Ausnahme: Genau sie
 * ersetzt das Paket.
 */
export function istKernPfad(pfad) {
  const p = pfad.replaceAll("\\", "/");
  if (SEITE_PFAD.test(p)) return false;
  return (
    p.startsWith("src-tauri/") ||
    p.startsWith("relay-proto/") ||
    p === "Cargo.toml" ||
    p === "Cargo.lock"
  );
}

/** Die Kern-Pfade aus einer Liste geänderter Dateien. */
export function geaenderteKernPfade(pfade) {
  return pfade.filter((p) => p && istKernPfad(p));
}

/** Geänderte Geräte-Seiten aus einer Liste geänderter Dateien. */
export function geaenderteSeiten(pfade) {
  return pfade.filter((p) => SEITE_PFAD.test(p));
}

/**
 * Zerlegt die Ausgabe von `git diff --no-renames -z --name-only`.
 *
 * Bewusst `-z`: Ohne das setzt git Namen mit Sonderzeichen in
 * Anführungszeichen (`"src-tauri/src/\303\274.rs"`), und die Kern-Regel sähe
 * den Pfad nicht mehr. Bewusst `--no-renames`: Sonst meldet ein Verschieben
 * nur das Ziel, und eine aus dem Kern herausgeschobene Datei fiele nicht auf.
 */
export function pfadeAusDiffZ(ausgabe) {
  return ausgabe.split("\0").filter(Boolean);
}

/** `v1.2.3` → `1.2.3`; alles andere ist ein Fehler. */
export function kernAusTag(tag) {
  const m = /^v(\d+\.\d+\.\d+)$/.exec(tag ?? "");
  if (!m) throw new Error(`Kein Release-Tag: „${tag}" (erwartet vX.Y.Z)`);
  return m[1];
}

/**
 * Liest die Seitenliste aus `tablet/assets.rs` — dieselbe Liste, gegen die
 * die App ein Paket prüft. Eine eigene Liste hier würde irgendwann abweichen.
 */
export function seitenAusAssetsRs(text) {
  const namen = [...text.matchAll(/^\s*[A-Z][A-Za-z]*\s*=>\s*"([a-z-]+\.html)"/gm)].map(
    (m) => m[1],
  );
  if (namen.length === 0) {
    throw new Error("Keine Seiten in assets.rs gefunden — diese Version kennt keine Seiten-Pakete");
  }
  if (new Set(namen).size !== namen.length) throw new Error("Seite doppelt in assets.rs");
  return namen;
}

/**
 * Prüft die Zeilen von `git ls-tree <commit> src-tauri/assets/`: Jede Seite
 * muss eine gewöhnliche Datei sein (Modus 100644). Ein Verweis (120000) oder
 * ein Untermodul (160000) unter einem Seitennamen ist ein Fehler — sonst
 * landete fremder Inhalt in einem signierten Paket.
 */
export function pruefeSeitenModi(lsTreeZ, namen) {
  const modus = new Map();
  for (const eintrag of lsTreeZ.split("\0").filter(Boolean)) {
    const m = /^(\d{6}) (\w+) [0-9a-f]+\t(.+)$/.exec(eintrag);
    if (m) modus.set(m[3], `${m[1]} ${m[2]}`);
  }
  for (const name of namen) {
    const art = modus.get(`${SEITEN_ORDNER}/${name}`);
    if (art !== "100644 blob") {
      throw new Error(`Seite ${name} ist keine gewöhnliche Datei (${art ?? "fehlt"})`);
    }
  }
}

/** Baut die Paket-Bytes (gzip-JSON) aus den Seitentexten. */
export function nutzlast({ kern, folge, commit, gebaut, seiten }) {
  if (!/^\d+\.\d+\.\d+$/.test(kern)) throw new Error(`Ungültige Kern-Version: ${kern}`);
  if (!Number.isInteger(folge) || folge < 1 || folge > MAX_FOLGE) {
    throw new Error(`Ungültige Folge: ${folge} (1 bis ${MAX_FOLGE}; der eingebaute Stand ist 0)`);
  }
  for (const [name, text] of Object.entries(seiten)) {
    if (typeof text !== "string" || text.trim() === "") {
      throw new Error(`Seite ${name} ist leer`);
    }
  }
  const json = JSON.stringify({ format: FORMAT, art: "seiten", kern, folge, commit, gebaut, seiten });
  return gzipSync(Buffer.from(json, "utf8"), { level: 9 });
}

function versionsZahlen(v) {
  return v.split(".").map(Number);
}

/** Vergleich zweier `X.Y.Z` — positiv, wenn `a` neuer ist. */
export function vergleicheVersion(a, b) {
  const [x, y] = [versionsZahlen(a), versionsZahlen(b)];
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] - y[i];
  return 0;
}

/** Dateiname des Pakets auf dem Server. */
export function paketName(kern, folge) {
  return `seiten-${kern}-${folge}.btsp`;
}

/**
 * Baut den Index aus der Liste der Dateien, die WIRKLICH auf dem Server
 * liegen. Ein Paket zählt nur mit seiner Signatur daneben. Je Kern-Version
 * steht die höchste Folge; behalten werden die jüngsten Kern-Versionen.
 *
 * Bewusst nicht aus dem alten Index fortgeschrieben: Der ist unsigniert, und
 * eine einmal falsche Zahl darin bliebe sonst für immer stehen. So kann der
 * Index nie auf ein Paket zeigen, das es nicht gibt.
 */
export function indexAusListe(dateinamen) {
  const da = new Set(dateinamen.map((n) => n.trim()).filter(Boolean));
  const je = new Map();
  for (const name of da) {
    const m = /^seiten-(\d+\.\d+\.\d+)-(\d{1,6})\.btsp$/.exec(name);
    if (!m || !da.has(`${name}.sig`)) continue;
    const folge = Number(m[2]);
    if (folge < 1 || folge > MAX_FOLGE || paketName(m[1], folge) !== name) continue;
    je.set(m[1], Math.max(je.get(m[1]) ?? 0, folge));
  }
  const pakete = [...je.entries()]
    .map(([kern, folge]) => ({ kern, folge }))
    .sort((a, b) => vergleicheVersion(b.kern, a.kern))
    .slice(0, KERN_VERSIONEN_IM_INDEX);
  return { format: FORMAT, pakete };
}

// ───────────────────────────── Kommandozeile ─────────────────────────────

function gitIn(repo) {
  return (...args) =>
    execFileSync("git", ["-C", repo, ...args], {
      encoding: "utf8",
      maxBuffer: 64 * 1024 * 1024,
      // Fehlertext von git nicht ins Protokoll durchreichen; die Meldung
      // formuliert der Aufrufer.
      stdio: ["ignore", "pipe", "pipe"],
    });
}

function argumente(liste) {
  const out = {};
  for (let i = 0; i < liste.length; i += 2) {
    const name = liste[i];
    if (!name?.startsWith("--") || liste[i + 1] === undefined) {
      throw new Error(`Unerwartetes Argument: ${name}`);
    }
    out[name.slice(2)] = liste[i + 1];
  }
  return out;
}

function brauche(a, ...namen) {
  for (const n of namen) if (!a[n]) throw new Error(`--${n} fehlt`);
}

/** Vollständige Commit-Kennung oder Fehler — nichts, was git als Option läse. */
function commitKennung(git, angabe) {
  if (!/^[0-9a-f]{40}$/.test(angabe ?? "")) {
    throw new Error(`Kein vollständiger Commit: „${angabe}"`);
  }
  git("cat-file", "-e", `${angabe}^{commit}`);
  return angabe;
}

/**
 * Prüft den Stand `commit` gegen den Tag und liefert alles, was das Paket
 * ausmacht. Liest nur über git.
 */
export function plan({ repo, tag, commit: angabe }) {
  const kern = kernAusTag(tag);
  const git = gitIn(repo);
  const commit = commitKennung(git, angabe);
  const tagCommit = git("rev-parse", "--verify", `refs/tags/${tag}^{commit}`).trim();
  // Der Stand muss vom Tag abstammen — sonst wäre „seit dem Tag" sinnlos.
  try {
    git("merge-base", "--is-ancestor", tagCommit, commit);
  } catch {
    throw new Error(`Der Stand ${commit.slice(0, 10)} stammt nicht vom Tag ${tag} ab.`);
  }
  const eingebaut = JSON.parse(git("show", `${tagCommit}:src-tauri/tauri.conf.json`)).version;
  if (eingebaut !== kern) {
    throw new Error(`Tag ${tag} trägt Version ${eingebaut}, erwartet ${kern}.`);
  }
  const geaendert = pfadeAusDiffZ(
    git("diff", "--no-renames", "-z", "--name-only", tagCommit, commit),
  );
  const kernPfade = geaenderteKernPfade(geaendert);
  if (kernPfade.length > 0) {
    throw new Error(
      `Der Zweig ändert gegenüber ${tag} Kern-Pfade — daraus entsteht kein Seiten-Paket:\n` +
        kernPfade.map((p) => `  ${p}`).join("\n") +
        "\nEin Fix am Kern braucht ein Release.",
    );
  }
  const folge = Number(git("rev-list", "--count", `${tagCommit}..${commit}`).trim());
  if (folge < 1) throw new Error(`Kein Commit seit ${tag} — nichts zu bauen.`);
  const seitenGeaendert = geaenderteSeiten(geaendert);
  if (seitenGeaendert.length === 0) {
    throw new Error(`Seit ${tag} ist keine Geräte-Seite geändert — nichts zu bauen.`);
  }
  // Die Seitenliste gilt, wie sie im TAG steht — das ist die Liste der
  // installierten Programmdatei.
  const namen = seitenAusAssetsRs(git("show", `${tagCommit}:${ASSETS_RS}`));
  pruefeSeitenModi(git("ls-tree", "-z", commit, `${SEITEN_ORDNER}/`), namen);
  const gebaut = git("show", "-s", "--format=%cI", commit).trim();
  return { kern, folge, commit, tagCommit, gebaut, namen, geaendert, seitenGeaendert };
}

/** Die Seitentexte eines Commits, direkt aus git — nie von der Platte. */
function seitenAusCommit(repo, commit, namen) {
  return Object.fromEntries(
    namen.map((n) => [
      n,
      gitIn(repo)("show", `${commit}:${SEITEN_ORDNER}/${n}`),
    ]),
  );
}

/** Nur für den Rust-Test: Seiten des Arbeitsverzeichnisses, ohne git. */
function seitenAusDateien(wurzel) {
  const namen = seitenAusAssetsRs(readFileSync(join(wurzel, ASSETS_RS), "utf8"));
  return Object.fromEntries(
    namen.map((n) => {
      const pfad = join(wurzel, SEITEN_ORDNER, n);
      if (!lstatSync(pfad).isFile()) throw new Error(`Seite ${n} ist keine gewöhnliche Datei`);
      return [n, readFileSync(pfad, "utf8")];
    }),
  );
}

function maskiert(text) {
  return text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}

/**
 * Was der Mensch vor der Freigabe liest: die geänderten Dateien und der
 * Unterschied der Seiten gegenüber dem Tag. Maskiert in `<pre>`, damit der
 * Inhalt einer Seite die Zusammenfassung nicht umformen kann.
 */
export function bericht({ repo, tag, commit }) {
  const p = plan({ repo, tag, commit });
  const git = gitIn(repo);
  const grenze = 300 * 1024;
  let diff = git(
    "diff", "--no-renames", "--no-ext-diff", "--no-textconv", p.tagCommit, p.commit,
    "--", ...p.seitenGeaendert,
  );
  const gekuerzt = diff.length > grenze;
  if (gekuerzt) diff = diff.slice(0, grenze);
  const andere = p.geaendert.filter((x) => !p.seitenGeaendert.includes(x));
  return [
    `## Seiten-Paket für ${tag}`,
    "",
    `Kern **${p.kern}**, Folge **${p.folge}**, Stand \`${p.commit}\``,
    "",
    `### Geänderte Seiten (${p.seitenGeaendert.length}) — das wird signiert`,
    "",
    `<pre>${maskiert(p.seitenGeaendert.join("\n"))}</pre>`,
    "",
    `### Sonst geändert (${andere.length}) — geht nicht ins Paket`,
    "",
    `<pre>${maskiert(andere.map((x) => JSON.stringify(x)).join("\n")) || "–"}</pre>`,
    "",
    "### Unterschied der Seiten gegenüber dem Tag",
    "",
    `<pre>${maskiert(diff)}</pre>`,
    gekuerzt ? "\n**Gekürzt.** Den vollen Unterschied vor der Freigabe im Zweig ansehen." : "",
    "",
    "**Nicht vergessen:** Der Fix muss auch nach `main` — sonst fehlt er den Cloud-Geräten und im nächsten Release.",
    "",
  ].join("\n");
}

function hauptprogramm([befehl, ...rest]) {
  const a = argumente(rest);
  if (befehl === "plan") {
    brauche(a, "repo", "tag", "commit");
    const p = plan(a);
    console.log(`kern=${p.kern}`);
    console.log(`folge=${p.folge}`);
    console.log(`commit=${p.commit}`);
    console.log(`tag_commit=${p.tagCommit}`);
    console.log(`paket=${paketName(p.kern, p.folge)}`);
    console.error(
      `Kern ${p.kern}, Folge ${p.folge}, Stand ${p.commit}\n` +
        `Geändert seit ${a.tag} (${p.geaendert.length} Dateien, davon ${p.seitenGeaendert.length} Seiten):\n` +
        p.geaendert.map((x) => `  ${JSON.stringify(x)}`).join("\n"),
    );
  } else if (befehl === "bericht") {
    brauche(a, "repo", "tag", "commit");
    console.log(bericht(a));
  } else if (befehl === "nutzlast") {
    brauche(a, "out");
    let bytes;
    if (a["aus-dateien"]) {
      brauche(a, "kern", "folge");
      bytes = nutzlast({
        kern: a.kern,
        folge: Number(a.folge),
        commit: a.commit ?? "",
        gebaut: "",
        seiten: seitenAusDateien(resolve(a["aus-dateien"])),
      });
    } else {
      brauche(a, "repo", "tag", "commit");
      // Prüft selbst noch einmal: Gebaut wird nie etwas Ungeprüftes.
      const p = plan(a);
      bytes = nutzlast({
        kern: p.kern,
        folge: p.folge,
        commit: p.commit,
        // Zeit des Commits, nicht die Uhr: Derselbe Stand ergibt so immer
        // dieselben Bytes, und ein erneuter Lauf passt zur Datei, die schon
        // auf dem Server liegt.
        gebaut: p.gebaut,
        seiten: seitenAusCommit(a.repo, p.commit, p.namen),
      });
    }
    writeFileSync(a.out, bytes);
    console.error(`${a.out}: ${bytes.length} Bytes`);
  } else if (befehl === "index") {
    brauche(a, "liste", "out");
    const neu = indexAusListe(readFileSync(a.liste, "utf8").split("\n"));
    writeFileSync(a.out, `${JSON.stringify(neu)}\n`);
    console.error(JSON.stringify(neu));
  } else {
    throw new Error(`Unbekannter Befehl: ${befehl} (plan | bericht | nutzlast | index)`);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    hauptprogramm(process.argv.slice(2));
  } catch (e) {
    console.error(`Fehler: ${e.message}`);
    process.exit(1);
  }
}
