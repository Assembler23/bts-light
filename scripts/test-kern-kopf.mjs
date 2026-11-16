/**
 * Bau-Skript des Kern-Kopfs (Spec hotfix-ohne-neustart, ADR 0100/0101) —
 * `scripts/build-kern-kopf.mjs` — und der Job `kern` im Release-Workflow.
 *
 * Warum: Der Kopf wird mit dem Updater-Schlüssel signiert und entscheidet,
 * welche Programmdatei ein Turnier-PC an die Stelle der laufenden setzt. Ein
 * falscher Hash wäre ein totes Update, ein fehlendes „MZ"-Prüfen eine Lücke.
 */
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { pruefeWechsel } from "./check-datenformat.mjs";
import { pruefen, schluesselLesen, updaterSchluessel } from "./verify-minisign.mjs";
import {
  exeName,
  istVersion,
  kernIndex,
  kernKopf,
  kopfName,
  vergleicheVersion,
} from "./build-kern-kopf.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const exe = Buffer.concat([Buffer.from("MZ"), Buffer.alloc(1000, 7)]);

// Kopf nennt Hash, Größe, Art und die Lesbarkeits-Grenze.
const kopf = kernKopf({ exe, version: "0.9.440", rueckwaertsLesbarAb: "0.9.436" });
assert.deepEqual(kopf, {
  format: 1,
  art: "kern",
  version: "0.9.440",
  exe_sha256: createHash("sha256").update(exe).digest("hex"),
  exe_groesse: 1002,
  rueckwaerts_lesbar_ab: "0.9.436",
});
assert.match(kopf.exe_sha256, /^[0-9a-f]{64}$/, "klein geschrieben, wie die App es liest");
// Grenze = eigene Version ist erlaubt (kein Rückweg zu Älterem).
assert.equal(
  kernKopf({ exe, version: "0.9.440", rueckwaertsLesbarAb: "0.9.440" }).rueckwaerts_lesbar_ab,
  "0.9.440",
);

// Kein Windows-Programm → kein Kopf.
for (const falsch of [Buffer.from("PK\x03\x04zip"), Buffer.from("#!/bin/sh"), Buffer.alloc(0), Buffer.from("M")]) {
  assert.throws(() => kernKopf({ exe: falsch, version: "1.2.3", rueckwaertsLesbarAb: "1.2.0" }), /kein Windows-Programm/);
}
// Krumme Versionen, Grenze in der Zukunft, fehlende Grenze.
for (const v of ["1.2", "v1.2.3", "1.2.3-rc1", "1.02.3", "", undefined]) {
  assert.throws(() => kernKopf({ exe, version: v, rueckwaertsLesbarAb: "1.0.0" }), /Ungültige Version/, String(v));
  assert.equal(istVersion(v), false, String(v));
}
assert.throws(() => kernKopf({ exe, version: "1.2.3", rueckwaertsLesbarAb: "1.2.4" }), /liegt nach der Version/);
for (const g of [undefined, "", "bald", null]) {
  assert.throws(() => kernKopf({ exe, version: "1.2.3", rueckwaertsLesbarAb: g }), /rueckwaerts_lesbar_ab/);
}

assert.ok(vergleicheVersion("0.9.100", "0.9.99") > 0);
assert.ok(vergleicheVersion("0.10.0", "0.9.999") > 0);
assert.deepEqual(kernIndex("0.9.440"), { format: 1, version: "0.9.440" });
assert.throws(() => kernIndex("neu"), /Ungültige Version/);
assert.equal(kopfName("0.9.440"), "kern-0.9.440.json");
assert.equal(exeName("0.9.440"), "bts-light-0.9.440.exe");

// Kommandozeile gegen die echte release/datenformat.json.
const dir = mkdtempSync(join(tmpdir(), "kern-kopf-"));
try {
  const skript = join(root, "scripts/build-kern-kopf.mjs");
  const lauf = (...args) => execFileSync(process.execPath, [skript, ...args], { stdio: "pipe" });
  const datenformat = JSON.parse(readFileSync(join(root, "release/datenformat.json"), "utf8"));
  const version = JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version;
  assert.ok(istVersion(datenformat.rueckwaerts_lesbar_ab), "datenformat.json nennt eine Version");
  assert.ok(
    vergleicheVersion(datenformat.rueckwaerts_lesbar_ab, version) <= 0,
    "die Grenze liegt nie nach der aktuellen Version",
  );
  assert.match(datenformat.abdruck, /^[0-9a-f]{64}$/);
  assert.ok(String(datenformat.begruendung ?? "").trim().length > 10, "die Grenze ist begründet");

  const exePfad = join(dir, "bts-light.exe");
  writeFileSync(exePfad, exe);
  const out = join(dir, "kopf.json");
  lauf("kopf", "--exe", exePfad, "--version", version, "--datenformat", join(root, "release/datenformat.json"), "--out", out);
  const geschrieben = JSON.parse(readFileSync(out, "utf8"));
  assert.equal(geschrieben.version, version);
  assert.equal(geschrieben.rueckwaerts_lesbar_ab, datenformat.rueckwaerts_lesbar_ab);
  assert.equal(geschrieben.exe_groesse, exe.length);

  writeFileSync(exePfad, "kein programm");
  assert.throws(() =>
    lauf("kopf", "--exe", exePfad, "--version", version, "--datenformat", join(root, "release/datenformat.json"), "--out", join(dir, "x.json")),
  );
  assert.throws(() => readFileSync(join(dir, "x.json")), "bei einem Fehler entsteht keine Datei");

  const index = join(dir, "kern.json");
  lauf("index", "--version", version, "--out", index);
  assert.equal(readFileSync(index, "utf8"), `{"format":1,"version":"${version}"}\n`);
} finally {
  rmSync(dir, { recursive: true, force: true });
}

// Signaturprüfung des Release-Jobs — an einer echten Ausgabe von
// `tauri signer sign` (Wegwerf-Schlüssel, dieselbe Probe wie in signatur.rs).
{
  const probeSchluessel =
    "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDRFRUUxQjBGMTVCQ0JGREQKUldUZHY3d1ZEeHZ1VGtjbkRnMnJYVXJtWHpIMDVZcWJtMFlVMGRycHAzNUdTeGJTQ0d0VU43YmkK";
  const probeSignatur =
    "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUZHY3d1ZEeHZ1VG5ORDZNaXU1MEx3MXdUU2N4eTI4bGFtN3NHK1pSZHdza2Z2Z1JWakNaalFBbmdnZXMrUyt3K0ZsaEJic3RVWlR3anpCTlY1aGZ0bHlZTnhIZHhpZkE0PQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk0ODEwMjg0CWZpbGU6cHJvYmUuYmluCm9XWFpkZHU2cE90U1Z2VGZkOWZKMUNPbUdHOHpRWlB5ckxqZmxsM1RKaXR3eUJyU1dId3c3UmlyMjRVaWRucmZlczRXUE9ZU3J1bndxYSs1QkE1T0FnPT0K";
  const daten = Buffer.from("bts-light Signatur-Probe\n");
  const k = schluesselLesen(probeSchluessel);
  pruefen(k, daten, probeSignatur);
  pruefen(k, daten, `${probeSignatur}\n`);
  assert.throws(() => pruefen(k, Buffer.from("bts-light Signatur-Probe!\n"), probeSignatur), /passt nicht zu den Daten/);
  // Der echte Updater-Schlüssel ist ein anderer.
  assert.throws(() => pruefen(updaterSchluessel(), daten, probeSignatur), /anderen Schlüssel/);
  for (const kaputt of ["", "kein base64 !!", "aGFsbG8="]) {
    assert.throws(() => pruefen(k, daten, kaputt), String(kaputt));
  }
  // Umetikettiert auf die Form ohne Vorab-Hash: abgelehnt.
  const zeilen = Buffer.from(probeSignatur, "base64").toString("utf8").split("\n");
  const bin = Buffer.from(zeilen[1], "base64");
  bin[1] = 0x64;
  zeilen[1] = bin.toString("base64");
  assert.throws(
    () => pruefen(k, daten, Buffer.from(zeilen.join("\n")).toString("base64")),
    /ohne Vorab-Hash/,
  );
}

// Wechsel der Datei gegenüber dem Zielzweig: Grenze sinkt nie, und ein neuer
// Abdruck braucht eine neue Begründung.
{
  const alt = { rueckwaerts_lesbar_ab: "0.9.436", abdruck: "a", begruendung: "Ausgangswert." };
  assert.deepEqual(pruefeWechsel(null, alt), [], "erster Stand");
  assert.deepEqual(pruefeWechsel(alt, alt), [], "nichts geändert");
  // Neues Feld mit Standardwert: Abdruck neu, Grenze bleibt, Begründung neu.
  assert.deepEqual(
    pruefeWechsel(alt, { ...alt, abdruck: "b", begruendung: "Neues Feld x mit Standardwert." }),
    [],
  );
  // Bruch: Grenze steigt.
  assert.deepEqual(
    pruefeWechsel(alt, { rueckwaerts_lesbar_ab: "0.9.440", abdruck: "b", begruendung: "Neue Variante." }),
    [],
  );
  // Nur die Zahl kopiert.
  assert.match(pruefeWechsel(alt, { ...alt, abdruck: "b" })[0], /Begründung nicht/);
  // Grenze gesenkt.
  assert.match(
    pruefeWechsel(alt, { ...alt, rueckwaerts_lesbar_ab: "0.9.430" })[0],
    /sinkt von 0\.9\.436 auf 0\.9\.430/,
  );
  assert.equal(
    pruefeWechsel(alt, { rueckwaerts_lesbar_ab: "0.9.99", abdruck: "b", begruendung: "x" }).length,
    1,
    "Zahlen, nicht Zeichen: 0.9.99 liegt vor 0.9.436",
  );
  assert.match(pruefeWechsel(alt, { abdruck: "b" })[0], /keine Version/);
}

// Der Job `kern` im Release-Workflow.
const wf = readFileSync(join(root, ".github/workflows/release.yml"), "utf8");
const ohneKommentare = wf.split("\n").filter((z) => !/^\s*#/.test(z)).join("\n");
const jobs = ohneKommentare.slice(ohneKommentare.indexOf("\njobs:"));
const ab = jobs.indexOf("\n  kern:");
assert.ok(ab > 0, "Job kern fehlt");
const kern = jobs.slice(ab);
// Darf den Installer-Weg nie aufhalten: läuft NACH publish und darf scheitern.
assert.match(kern, /needs: \[?publish/);
assert.match(kern, /continue-on-error: true/);
assert.match(kern, /startsWith\(github\.ref, 'refs\/tags\/'\)/);
// Zeitlimit: Der Lauf hält die Release-Sperre.
assert.match(kern, /timeout-minutes: \d+/);
// Der Installer wird gegen seine Signatur geprüft, BEVOR etwas entnommen
// oder signiert wird — sonst stellte der Job für eine ausgetauschte Datei
// eine echte Signatur aus.
const pruefung = kern.indexOf("verify-minisign.mjs");
assert.ok(pruefung > 0, "Installer-Prüfung fehlt");
assert.ok(pruefung < kern.indexOf("7z x"), "erst prüfen, dann entpacken");
assert.ok(pruefung < kern.indexOf("signer sign"), "erst prüfen, dann signieren");
assert.match(kern, /\*_"\$\{VERSION\}"_\*-setup\.exe/, "der Installer gehört zu dieser Version");
// Erst prüfen, dann signieren; erst die Nutzlast, dann der Index.
assert.ok(kern.indexOf("build-kern-kopf.mjs kopf") < kern.indexOf("signer sign"));
assert.ok(kern.indexOf("Kern hochladen") > 0 && kern.indexOf("Kern hochladen") < kern.indexOf("Index hochladen"));
assert.doesNotMatch(kern, /--delete/);
// Kein `${{ … }}` in einem run-Block.
for (const block of kern.matchAll(/run: \|\n((?:\s{10,}.*\n?|\n)+)/g)) {
  assert.doesNotMatch(block[1], /\$\{\{/, block[1].slice(0, 80));
}
// Die übrigen Jobs hängen nicht von `kern` ab.
assert.doesNotMatch(jobs.slice(0, ab), /needs:.*\bkern\b/);

console.log("OK — Kern-Kopf: Bau-Skript und Release-Job geprüft.");
