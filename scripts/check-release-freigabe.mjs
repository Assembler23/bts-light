// Prüft, ob der Workflow „Release auslösen" den Versions-Tag setzen darf.
//
// Warum es das gibt: Das Ruleset „Release-Tags nur durch Admins" sperrt
// `refs/tags/v*`. Wer nur Schreibrecht hat, konnte einen Release deshalb nie
// selbst anstoßen — jeder Tag hing an einer Übergabe an den Admin, und genau
// an dieser Übergabe blieben Versionen liegen (siehe release-faellig.yml).
// Statt mehr Rechte an Personen zu vergeben, taggt jetzt ein Workflow mit
// einem eigenen Deploy-Key. Diese Datei ist sein Türsteher: Der Workflow darf
// nur taggen, was hier durchgeht.
//
// Was NICHT hier entschieden wird: OB ein Release gerade passt. Der Tag löst
// ein Auto-Update auf allen laufenden Turnier-PCs aus; das weiß nur ein
// Mensch, und der drückt den Knopf (und tippt die Version zur Bestätigung).
//
// Aufruf (im Workflow):  node scripts/check-release-freigabe.mjs
// Testbar: scripts/test-release-freigabe.mjs prüft pruefeFreigabe() ohne
// git/Netz.
//
// @see .github/workflows/release-ausloesen.yml
// @see docs/release.md („Release per Knopf auslösen")

import { execFileSync } from "node:child_process";
import { appendFileSync, readFileSync } from "node:fs";
import { cargoVersion, tauriVersion, istHauptmodul } from "./check-version-tagged.mjs";

/**
 * Die reine Regel — ohne git, ohne Netz, damit testbar. Sammelt ALLE
 * Hinderungsgründe statt beim ersten abzubrechen: Wer den Knopf drückt, soll
 * nicht dreimal nacheinander je einen Fehler abholen.
 *
 * @param {object} e
 * @param {string}      e.ref         Ref des Laufs, z. B. "refs/heads/main"
 * @param {string}      e.sha         Commit des Laufs
 * @param {string|null} e.mainSpitze  aktuelle Spitze von main auf dem Server
 * @param {string}      e.eingabe     von Hand getippte Version (Bestätigung)
 * @param {string|null} e.cargo       Version aus src-tauri/Cargo.toml
 * @param {string|null} e.tauri       Version aus src-tauri/tauri.conf.json
 * @param {string|null} e.paket       Version aus package.json
 * @param {boolean}     e.tagVorhanden  gibt es v<Version> schon auf dem Server?
 * @param {{status:string, conclusion:string|null, headSha:string}|null} e.ci
 *        maßgeblicher CI-Lauf auf main (siehe waehleCi), null = keiner gefunden
 * @param {string[]|null} [e.seitCi]
 *        Dateien, die sich zwischen dem Commit des CI-Laufs und `sha` geändert
 *        haben; null = nicht ermittelbar. Nur von Belang, wenn der Lauf zu
 *        einem älteren Commit gehört.
 * @returns {{ok: boolean, tag: string|null, fehler: string[]}}
 */
export function pruefeFreigabe({ ref, sha, mainSpitze, eingabe, cargo, tauri, paket, tagVorhanden, ci, seitCi = null }) {
  const fehler = [];

  // Nur main: Dort ist jede Änderung durch einen PR gegangen — auch eine an
  // diesem Skript und am Workflow selbst.
  if (ref !== "refs/heads/main") {
    fehler.push(`Der Lauf muss auf main gestartet werden (war: ${ref || "unbekannt"}).`);
  }

  // Getaggt wird der Commit des Laufs. Ist main inzwischen weiter, gehörte
  // die grüne CI unten womöglich zu einem ANDEREN Commit als dem getaggten.
  if (!mainSpitze || mainSpitze !== sha) {
    fehler.push("main hat sich seit dem Start bewegt (oder ist nicht lesbar) — bitte neu auslösen.");
  }

  let version = null;
  if (!cargo || !tauri || !paket) {
    fehler.push("Version nicht lesbar (Cargo.toml, tauri.conf.json oder package.json).");
  } else if (cargo !== tauri || cargo !== paket) {
    fehler.push(`Versionen uneinig: Cargo.toml ${cargo}, tauri.conf.json ${tauri}, package.json ${paket}.`);
  } else if (!/^\d+\.\d+\.\d+$/.test(cargo)) {
    // Der Tag wird später in Shell-Aufrufen und URLs verwendet — nur die
    // schlichte Form zulassen.
    fehler.push(`Version „${cargo}" hat nicht die Form X.Y.Z.`);
  } else {
    version = cargo;
  }

  // Die getippte Version ist die Bestätigung „ich weiß, was ich rausgebe" —
  // ein versehentlicher Klick im Actions-Tab löst sonst ein Auto-Update aus.
  const getippt = String(eingabe ?? "").trim().replace(/^v/i, "");
  if (version && getippt !== version) {
    fehler.push(`Bestätigung passt nicht: getippt „${getippt}", auf main steht ${version}.`);
  }

  if (version && tagVorhanden) {
    fehler.push(`Tag v${version} existiert bereits — diese Version ist schon veröffentlicht.`);
  }

  // Jüngster CI-Lauf auf main — und er muss zu DIESEM Commit gehören. Ohne
  // den Abgleich gäbe es ein Fenster: Merge, sofort auslösen, der Lauf für den
  // neuen Commit ist noch nicht angelegt, die Abfrage liefert den grünen Lauf
  // des Vorgängers — und ein ungeprüfter Stand ginge an alle Turnier-PCs.
  //
  // Einzige Ausnahme: ci.yml überspringt reine Doku-Pushes (paths-ignore).
  // Kamen nach dem geprüften Commit nur solche Dateien, gibt es für `sha` nie
  // einen eigenen Lauf, und der ältere deckt den Code-Stand vollständig.
  if (ci && ci.headSha !== sha) {
    if (!seitCi) {
      fehler.push("Der jüngste CI-Lauf gehört zu einem anderen Commit, und der Abstand ist nicht ermittelbar.");
    } else {
      const ungeprueft = seitCi.filter((p) => !ciUeberspringt(p));
      if (ungeprueft.length > 0) {
        fehler.push(
          `Für diesen Commit gibt es noch keinen CI-Lauf (seit dem letzten geändert: ${ungeprueft.slice(0, 3).join(", ")}` +
            `${ungeprueft.length > 3 ? " …" : ""}) — kurz warten und neu auslösen. Kommt keiner ` +
            `(Merge durch einen Workflow, z. B. Dependabot): „gh workflow run ci.yml --ref main".`,
        );
      }
    }
  }
  if (!ci) {
    fehler.push("Kein CI-Lauf auf main gefunden.");
  } else if (ci.status !== "completed") {
    fehler.push("Die CI auf main läuft noch — nach ihrem Ende neu auslösen.");
  } else if (ci.conclusion !== "success") {
    fehler.push(`Die CI auf main ist nicht grün (${ci.conclusion ?? "ohne Ergebnis"}).`);
  }

  const ok = fehler.length === 0;
  return { ok, tag: ok ? `v${version}` : null, fehler };
}

/**
 * `paths-ignore` des main-Pushes in ci.yml — für diese Pfade startet KEIN
 * CI-Lauf. Muss mit ci.yml übereinstimmen; der Test vergleicht beide Listen.
 */
export const CI_PATHS_IGNORE = ["**/*.md", "docs/**", ".github/ISSUE_TEMPLATE/**", "LICENSE"];

/** Überspringt die CI auf main einen Push, der nur diesen Pfad anfasst? */
export function ciUeberspringt(pfad) {
  return (
    pfad.endsWith(".md") ||
    pfad.startsWith("docs/") ||
    pfad.startsWith(".github/ISSUE_TEMPLATE/") ||
    pfad === "LICENSE"
  );
}

/**
 * Welcher CI-Lauf gilt? Die Regel will den Lauf für den zu taggenden Commit.
 * Normalerweise ist das der jüngste Push-Lauf. Merged aber ein Workflow mit
 * dem GITHUB_TOKEN (Dependabot-Automerge), startet GitHub KEINEN Push-Lauf —
 * dann zählt ein von Hand gestarteter Lauf (`workflow_dispatch` auf main),
 * sofern er genau diesen Commit geprüft hat. Sonst bleibt es beim Push-Lauf,
 * damit die Doku-Ausnahme (`seitCi`) wie bisher greift.
 *
 * @param {{headSha:string}|null} push  jüngster Push-Lauf auf main
 * @param {{headSha:string}|null} hand  jüngster Hand-Lauf auf main
 * @param {string} sha                  zu taggender Commit
 */
export function waehleCi(push, hand, sha) {
  if (push && push.headSha === sha) return push;
  if (hand && sha && hand.headSha === sha) return hand;
  return push ?? null;
}

/** Version aus package.json. */
export function paketVersion(inhalt) {
  try {
    return JSON.parse(inhalt).version ?? null;
  } catch {
    return null;
  }
}

// ── CLI ─────────────────────────────────────────────────────────────────────
if (istHauptmodul(import.meta.url, process.argv[1])) {
  const lauf = (cmd, ...args) => execFileSync(cmd, args, { encoding: "utf8" }).trim();
  // Ein Lesefehler darf nie als „alles gut" durchgehen: null/leer führt in
  // pruefeFreigabe() immer zu einem Fehler, nie zu einer Freigabe.
  const versuch = (fn) => {
    try {
      return fn();
    } catch (e) {
      console.error(`  (Abfrage fehlgeschlagen: ${e.message.split("\n")[0]})`);
      return null;
    }
  };

  const cargo = cargoVersion(readFileSync("src-tauri/Cargo.toml", "utf8"));
  const tauri = tauriVersion(readFileSync("src-tauri/tauri.conf.json", "utf8"));
  const paket = paketVersion(readFileSync("package.json", "utf8"));

  const mainSpitze = versuch(() => lauf("git", "ls-remote", "origin", "refs/heads/main").split(/\s+/)[0] || null);

  // Schlägt die Tag-Abfrage fehl, gilt der Tag als vorhanden (= Sperre).
  const tagZeile = versuch(() => lauf("git", "ls-remote", "--tags", "origin", `refs/tags/v${cargo}`));
  const tagVorhanden = tagZeile === null ? true : tagZeile.length > 0;

  const juengster = (event) =>
    versuch(() => {
      const laeufe = JSON.parse(
        lauf("gh", "run", "list", "--workflow", "ci.yml", "--branch", "main", "--event", event,
          "--limit", "1", "--json", "status,conclusion,headSha"),
      );
      return laeufe[0] ?? null;
    });

  const sha = process.env.GITHUB_SHA ?? "";
  const ci = waehleCi(juengster("push"), juengster("workflow_dispatch"), sha);
  // Abstand zwischen geprüftem und zu taggendem Commit. Der CI-Commit muss ein
  // Vorfahre sein (braucht die volle Historie: fetch-depth 0 im Workflow).
  let seitCi = null;
  if (ci?.headSha && sha && ci.headSha !== sha) {
    seitCi = versuch(() => {
      lauf("git", "merge-base", "--is-ancestor", ci.headSha, sha);
      return lauf("git", "diff", "--name-only", ci.headSha, sha).split("\n").filter(Boolean);
    });
  }

  const u = pruefeFreigabe({
    ref: process.env.GITHUB_REF ?? "",
    sha,
    seitCi,
    mainSpitze,
    eingabe: process.env.EINGABE ?? "",
    cargo,
    tauri,
    paket,
    tagVorhanden,
    ci,
  });

  if (!u.ok) {
    console.error("✗ Kein Release — es wurde KEIN Tag gesetzt:");
    for (const f of u.fehler) console.error(`  - ${f}`);
    process.exit(1);
  }
  console.log(`✓ Freigabe: ${u.tag} auf ${process.env.GITHUB_SHA}`);
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `tag=${u.tag}\n`);
}
