// Meldet, wenn Arbeit auf main schon zu lange auf ein Release wartet — und
// damit auf keinem Turnier-PC ankommt.
//
// Warum es das gibt: Der Versionssprung passiert inzwischen INNERHALB der
// Feature-Commits ("… (v0.9.199)"), es gibt keinen eigenen "Release vX.Y.Z"-
// Commit mehr, der zum Taggen auffordert. Ohne Tag fällt nichts auf: main ist
// grün, kein Job schlägt fehl, nur Tilo sieht seine Features nicht. Zweimal
// passiert — einmal blieb v0.9.186 liegen, dann zwölf Versionen (0.9.188 bis
// 0.9.199) zwischen dem Tag von v0.9.187 (12.08. 13:43) und der Meldung am
// 13.08. — also in **19 Stunden**, nicht über Tage.
//
// ── Zwei Grenzen, weil je eine allein den Vorfall verschluckt hätte ────────
// Erster Entwurf: "ist die aktuelle Version getaggt, und wie alt ist sie?" —
// verschluckt es, weil bei schnellen Sprüngen die jüngste Version immer jung
// ist (0.9.199 war zur Meldung 1,4 h alt). Jede wird von der nächsten
// überschrieben, bevor eine Uhr greift.
//
// Zweiter Entwurf: Alter des ältesten unveröffentlichten Commits. Besser, aber
// auch das schwieg — der älteste war 19 h alt, unter der 24-h-Grenze.
//
// Deshalb ZWEI Maßstäbe, gemessen statt geraten:
//   * die Uhr   fängt "liegt liegen"      (ältester offener Commit ≥ 24 h)
//   * die Menge fängt "hat sich gestapelt" (≥ 5 unveröffentlichte Sprünge)
// Beim echten Vorfall greift nur die zweite.
//
// Was dieser Check NICHT tut: taggen. Der Tag löst ein Auto-Update auf allen
// laufenden Turnier-PCs aus; ob das gerade passt, weiß nur ein Mensch. Der
// Ablauf bleibt unverändert — Tilo meldet, wann getaggt wird. Dieser Check ist
// das Netz für den Fall, dass es niemand meldet.
//
// Aufruf:  node scripts/check-version-tagged.mjs
// Testbar: scripts/test-version-tagged.mjs prüft bewerte() ohne git/Netz.

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

/**
 * Ab wann gilt unveröffentlichte Arbeit als vergessen?
 *
 * 24 Stunden, bewusst großzügig: getaggt wird erst, wenn Tilo es meldet, und
 * zwischen Merge und Meldung darf ein Tag liegen. Ein Check, der routinemäßig
 * rot ist, wird ignoriert — dann hätte man ihn umsonst gebaut.
 */
export const GRENZE_STUNDEN = 24;

/**
 * Ab wie vielen unveröffentlichten Versionssprüngen ist ein Release fällig,
 * unabhängig von der Uhr?
 *
 * Gemessen am echten Vorfall: zwischen v0.9.187 (getaggt 12.08. 13:43) und der
 * Meldung lagen zwölf Sprünge in **19 Stunden**. Eine reine Zeitgrenze von 24 h
 * hätte geschwiegen — bei diesem Tempo bildet sich ein Stapel schneller, als
 * eine Uhr ihn bemerkt. Fünf ist der Punkt, ab dem "ein Haufen" kein normales
 * Einzel-Release-Fenster mehr ist.
 */
export const GRENZE_SPRUENGE = 5;

/**
 * Pfade, deren Änderung KEIN Release nötig macht: sie landen nicht im Installer.
 * Dieselbe Haltung wie `paths-ignore` in ci.yml — sonst wäre der Check nach
 * jeder Doku-Ergänzung rot und würde zum Rauschen.
 */
export function istAuslieferbar(pfad) {
  if (pfad.startsWith("docs/")) return false;
  if (pfad.startsWith(".github/")) return false;
  if (pfad.endsWith(".md")) return false;
  if (pfad === "LICENSE") return false;
  // Tests werden nicht mitgeliefert.
  if (pfad.startsWith("src-tauri/tests/")) return false;
  // scripts/test-* und scripts/check-* sind CI-Helfer und landen nicht im
  // Installer. Namenskonvention, damit die Liste nicht pro Datei waechst —
  // dieser Check selbst ist der erste Fall (er wuerde sonst die Uhr fuer sich
  // selbst starten).
  if (/^scripts\/(test|check)-/.test(pfad)) return false;
  return true;
}

/**
 * Die reine Regel — ohne git, ohne Dateisystem, damit testbar.
 *
 * @param {object} e
 * @param {string}      e.cargo   Version aus src-tauri/Cargo.toml
 * @param {string}      e.tauri   Version aus src-tauri/tauri.conf.json
 * @param {string|null} e.letzterTag  neuester Tag auf main, z. B. "v0.9.199"
 * @param {{sha:string,alterStunden:number,titel:string}[]} e.offen
 *        auslieferbare Commits nach dem letzten Tag, neueste zuerst
 * @param {string[]} [e.spruenge]  unveroeffentlichte Versionsspruenge, z. B. ["0.9.188","0.9.199"]
 * @param {number} [e.grenzeStunden]
 * @param {number} [e.grenzeSpruenge]
 * @returns {{ok: boolean, code: string, text: string}}
 */
export function bewerte({
  cargo,
  tauri,
  letzterTag,
  offen,
  spruenge = [],
  grenzeStunden = GRENZE_STUNDEN,
  grenzeSpruenge = GRENZE_SPRUENGE,
}) {
  if (!cargo || !tauri) {
    return { ok: false, code: "version-fehlt", text: "Version nicht lesbar (Cargo.toml oder tauri.conf.json)." };
  }

  // Prüfung 2 des Release-Ablaufs. Driften die beiden, ist der nächste Tag
  // zwangsläufig falsch — und das merkt heute niemand vor dem Taggen.
  if (cargo !== tauri) {
    return {
      ok: false,
      code: "versionen-uneinig",
      text: `Cargo.toml sagt ${cargo}, tauri.conf.json sagt ${tauri}. Vor dem naechsten Tag angleichen.`,
    };
  }

  if (offen.length === 0) {
    return { ok: true, code: "alles-geliefert", text: `Nichts Unveroeffentlichtes auf main (letzter Tag: ${letzterTag ?? "keiner"}).` };
  }

  const aeltester = offen[offen.length - 1];
  const zuAlt = aeltester.alterStunden >= grenzeStunden;
  const zuViele = spruenge.length >= grenzeSpruenge;

  // ZWEI Grenzen, weil eine allein den echten Vorfall verschluckt hat: die Uhr
  // fasst "liegt liegen", die Menge fasst "es hat sich gestapelt". Bei zwölf
  // Spruengen in 19 Stunden greift nur die zweite.
  if (!zuAlt && !zuViele) {
    return {
      ok: true,
      code: "frisch",
      text:
        `${offen.length} auslieferbare(r) Commit(s) seit ${letzterTag ?? "dem Anfang"} noch ohne Release, ` +
        `aeltester ${Math.floor(aeltester.alterStunden)} h alt (Grenze ${grenzeStunden} h), ` +
        `${spruenge.length} Versionssprung/-spruenge (Grenze ${grenzeSpruenge}) — im Rahmen.`,
    };
  }

  const tage = Math.floor(aeltester.alterStunden / 24);
  const grund = zuViele
    ? `${spruenge.length} unveroeffentlichte Versionsspruenge (${spruenge.join(", ")})`
    : `aelteste Arbeit ${tage} Tag(e) alt`;
  const liste = offen
    .slice(-5)
    .reverse()
    .map((c) => `    ${c.sha}  ${c.titel}`)
    .join("\n");

  return {
    ok: false,
    code: "release-faellig",
    text:
      `Release faellig: ${grund}. ${offen.length} auslieferbare(r) Commit(s) seit ` +
      `${letzterTag ?? "dem Anfang"} erreichen keinen Turnier-PC.\n` +
      `  aelteste offene:\n${liste}\n` +
      `  Wenn es raus soll:\n` +
      `    git tag -a v${cargo} origin/main -m "BTS Light v${cargo}" && git push origin v${cargo}`,
  };
}

/** Version aus Cargo.toml (erste `version = "…"`-Zeile). */
export function cargoVersion(inhalt) {
  return inhalt.match(/^version\s*=\s*"([^"]+)"/m)?.[1] ?? null;
}

/** Version aus tauri.conf.json. */
export function tauriVersion(inhalt) {
  try {
    return JSON.parse(inhalt).version ?? null;
  } catch {
    return null;
  }
}

/**
 * Wird DIESE Datei gerade direkt aufgerufen (statt importiert)?
 *
 * Der naheliegende Vergleich `import.meta.url === \`file://${process.argv[1]}\``
 * geht nur auf POSIX auf ("/home/…" liefert den dritten Slash selbst). Auf
 * **Windows** ist `argv[1]` ein "C:\…"-Pfad mit Backslashes, die URL aber
 * "file:///C:/…" — der Vergleich schlaegt immer fehl, das Skript tut nichts
 * und endet mit exit 0. Genau die stille gruene Meldung, gegen die dieser
 * Check gebaut wurde, und der lokale Aufruf aus docs/release.md laeuft auf
 * einem Windows-Rechner.
 *
 * Deshalb beide Seiten auf dieselbe Form bringen: URL-Praefix weg,
 * Prozentzeichen aufloesen, Backslashes zu Slashes, fuehrenden Slash vor dem
 * Laufwerksbuchstaben entfernen.
 */
export function istHauptmodul(metaUrl, argv1) {
  if (!metaUrl || !argv1) return false;
  const norm = (s) => {
    let p = String(s);
    try {
      p = decodeURIComponent(p);
    } catch {
      // Kaputtes Encoding: unveraendert weiterverwenden.
    }
    return p
      .replace(/^file:\/\//, "")
      .replace(/\\/g, "/")
      .replace(/^\/+/, "/")
      .replace(/^\/([a-zA-Z]:)/, "$1");
  };
  return norm(metaUrl) === norm(argv1);
}

/**
 * Referenz, gegen die geprueft wird: `origin/main`, sonst das lokale `main`.
 *
 * `execFileSync` **wirft** bei einem git-Exit ≠ 0, ein Ausdruck wie
 * `git(…).length ? "origin/main" : "main"` kann seinen false-Zweig also nie
 * nehmen — der Fallback war toter Code, und ein Klon ohne `origin/main`
 * (frischer Worktree, anders benanntes Remote) starb mit rohem Stacktrace
 * statt auf `main` auszuweichen.
 */
export function hauptReferenz(git) {
  try {
    git("rev-parse", "--verify", "origin/main");
    return "origin/main";
  } catch {
    return "main";
  }
}

/**
 * Sammelt die unveroeffentlichten Commits des Bereichs und die darin
 * enthaltenen Versionsspruenge. `git` wird hereingereicht, damit die Logik
 * ohne echtes Repo testbar bleibt.
 *
 * **Merge-Commits:** `git show --name-only` gibt fuer einen Merge KEINE
 * Dateien aus (er waere als "reine Doku" durchgerutscht) und `git show -U0`
 * einen combined diff, dessen "++version"-Zeilen `/^\+version/` nie trifft —
 * ein per Merge-Commit gelandeter Versionssprung fehlte also in beiden
 * Zaehlungen und der Check bliebe gruen. Heute mergt das Repo per Squash;
 * die Zaehlung darf davon aber nicht abhaengen. Darum `--first-parent` plus
 * `git diff sha^ sha` (bei einem Merge = was er auf main gebracht hat).
 *
 * @returns {{offen: {sha:string,titel:string,alterStunden:number}[], spruenge: string[]}}
 */
export function sammleOffene(git, bereich, jetzt = Date.now()) {
  const rohe = git("log", "--first-parent", "--format=%h%x09%cI%x09%s", bereich)
    .split("\n")
    .filter(Boolean);

  const offen = [];
  const spruenge = [];
  for (const zeile of rohe) {
    const [sha, iso, titel] = zeile.split("\t");
    let dateien = [];
    try {
      dateien = git("diff", "--name-only", `${sha}^`, sha).split("\n").filter(Boolean);
    } catch {
      continue; // z. B. Root-Commit ohne Elternteil
    }
    if (!dateien.some(istAuslieferbar)) continue; // reine Doku/Tests
    offen.push({ sha, titel, alterStunden: (jetzt - Date.parse(iso)) / 3_600_000 });

    // Versionsspruenge: jede Version, die im offenen Bereich erstmals in
    // Cargo.toml auftaucht — der Maszstab "es hat sich gestapelt".
    if (!dateien.includes("src-tauri/Cargo.toml")) continue;
    let diff = "";
    try {
      diff = git("diff", "-U0", `${sha}^`, sha, "--", "src-tauri/Cargo.toml");
    } catch {
      continue;
    }
    for (const m of diff.matchAll(/^\+version\s*=\s*"([^"]+)"/gm)) {
      if (!spruenge.includes(m[1])) spruenge.push(m[1]);
    }
  }
  return { offen, spruenge };
}

// ── CLI ─────────────────────────────────────────────────────────────────────
if (istHauptmodul(import.meta.url, process.argv[1])) {
  const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();

  const cargo = cargoVersion(readFileSync("src-tauri/Cargo.toml", "utf8"));
  const tauri = tauriVersion(readFileSync("src-tauri/tauri.conf.json", "utf8"));

  // Referenz ist immer main, nicht der ausgecheckte Branch — der Check soll
  // dasselbe sagen, egal von wo er läuft.
  const main = hauptReferenz(git);

  let letzterTag = null;
  try {
    letzterTag = git("describe", "--tags", "--abbrev=0", main);
  } catch {
    letzterTag = null; // noch kein Tag im Repo
  }

  const bereich = letzterTag ? `${letzterTag}..${main}` : main;
  const { offen, spruenge } = sammleOffene(git, bereich);

  const u = bewerte({ cargo, tauri, letzterTag, offen, spruenge });
  console.log(
    `Version: ${cargo ?? "?"} (Cargo) / ${tauri ?? "?"} (tauri.conf), letzter Tag: ${letzterTag ?? "keiner"}, ` +
      `offen: ${offen.length} Commit(s) / ${spruenge.length} Sprung(-spruenge)`,
  );
  console.log(`${u.ok ? "✓" : "✗"} [${u.code}] ${u.text}`);
  process.exit(u.ok ? 0 : 1);
}
