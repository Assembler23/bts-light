// Baut den Kern-Kopf für den Kernwechsel ohne Installer (Spec
// docs/features/hotfix-ohne-neustart.md, ADR 0100/0101).
//
// Der Kopf ist ein kleines JSON, das die Programmdatei per SHA-256 und Größe
// benennt und sagt, ab welcher älteren Version die Daten dieser Version noch
// lesbar sind. SIGNIERT wird der Kopf (im Release-Workflow), nicht die exe —
// die App prüft die exe gegen den Kopf.
//
// Aufrufe (Workflow .github/workflows/release.yml, Job `kern`):
//   kopf  --exe <datei> --version X.Y.Z --datenformat release/datenformat.json --out <datei>
//   index --version X.Y.Z --out <datei>
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Einziges Kopf-Format, das die App kennt. */
export const FORMAT = 1;

const VERSION = /^(0|[1-9]\d{0,8})\.(0|[1-9]\d{0,8})\.(0|[1-9]\d{0,8})$/;

/** Ist `text` eine Version `X.Y.Z`, wie die App sie liest? */
export function istVersion(text) {
  return typeof text === "string" && VERSION.test(text);
}

/** Vergleich zweier `X.Y.Z` — positiv, wenn `a` neuer ist. */
export function vergleicheVersion(a, b) {
  const [x, y] = [a.split(".").map(Number), b.split(".").map(Number)];
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] - y[i];
  return 0;
}

/**
 * Baut den Kopf. Wirft, wenn die Datei kein Windows-Programm ist oder die
 * Angaben nicht zusammenpassen — lieber kein Kopf als ein falscher, denn der
 * wird signiert.
 */
export function kernKopf({ exe, version, rueckwaertsLesbarAb }) {
  if (!istVersion(version)) throw new Error(`Ungültige Version: ${version}`);
  if (!(exe instanceof Uint8Array) || exe.length < 2 || exe[0] !== 0x4d || exe[1] !== 0x5a) {
    throw new Error("Die Datei ist kein Windows-Programm (kein MZ am Anfang)");
  }
  if (!istVersion(rueckwaertsLesbarAb)) {
    throw new Error(`Ungültige Angabe rueckwaerts_lesbar_ab: ${rueckwaertsLesbarAb}`);
  }
  if (vergleicheVersion(rueckwaertsLesbarAb, version) > 0) {
    throw new Error(
      `rueckwaerts_lesbar_ab (${rueckwaertsLesbarAb}) liegt nach der Version ${version}`,
    );
  }
  return {
    format: FORMAT,
    art: "kern",
    version,
    exe_sha256: createHash("sha256").update(exe).digest("hex"),
    exe_groesse: exe.length,
    rueckwaerts_lesbar_ab: rueckwaertsLesbarAb,
  };
}

/** Der unsignierte Index: nur die Zahl der neuesten Version. */
export function kernIndex(version) {
  if (!istVersion(version)) throw new Error(`Ungültige Version: ${version}`);
  return { format: FORMAT, version };
}

/** Dateinamen auf dem Server. */
export const kopfName = (version) => `kern-${version}.json`;
export const exeName = (version) => `bts-light-${version}.exe`;

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

function hauptprogramm([befehl, ...rest]) {
  const a = argumente(rest);
  if (befehl === "kopf") {
    for (const n of ["exe", "version", "datenformat", "out"]) if (!a[n]) throw new Error(`--${n} fehlt`);
    const datenformat = JSON.parse(readFileSync(a.datenformat, "utf8"));
    const kopf = kernKopf({
      exe: readFileSync(a.exe),
      version: a.version,
      rueckwaertsLesbarAb: datenformat.rueckwaerts_lesbar_ab,
    });
    // Genau diese Bytes werden signiert und von der App geprüft.
    writeFileSync(a.out, `${JSON.stringify(kopf)}\n`);
    console.error(JSON.stringify(kopf));
  } else if (befehl === "index") {
    for (const n of ["version", "out"]) if (!a[n]) throw new Error(`--${n} fehlt`);
    writeFileSync(a.out, `${JSON.stringify(kernIndex(a.version))}\n`);
  } else {
    throw new Error(`Unbekannter Befehl: ${befehl} (kopf | index)`);
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
