// Eine Quelle für die Dateien auf dem Pi.
//
// Kiosk-Skript, CEC-Dienst und Hinweisseite standen früher als Heredocs in
// pi/setup-monitor.sh. Seit dem Umbau liegen sie als echte Dateien unter
// pi/paket/ — aus demselben Baum entsteht das Image und später das
// Fernupdate-Paket (Spec docs/features/pi-fernupdate.md, ADR 0103). Dieser
// Test hält Liste, Ordner und Einrichtungsskript beieinander: Eine Datei, die
// nur an einer der drei Stellen auftaucht, käme sonst still nicht auf den Pi.
import { readFileSync, readdirSync, statSync, mkdtempSync, mkdirSync, writeFileSync, copyFileSync, chmodSync, symlinkSync, rmSync, realpathSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join, delimiter } from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";

const paket = fileURLToPath(new URL("../pi/paket/", import.meta.url));
const setup = readFileSync(new URL("../pi/setup-monitor.sh", import.meta.url), "utf8");

// ── INHALT lesen ────────────────────────────────────────────────────────
const liste = readFileSync(join(paket, "INHALT"), "utf8")
  .split("\n")
  .map((z) => z.trim())
  .filter((z) => z && !z.startsWith("#"))
  .map((z) => {
    const m = /^(644|755) ([a-z0-9][a-z0-9./-]*)$/.exec(z);
    assert.ok(m, `INHALT: Zeile nicht lesbar: „${z}"`);
    assert.ok(!m[2].includes(".."), `INHALT: „..": ${z}`);
    return { modus: m[1], pfad: m[2] };
  });
assert.ok(liste.length >= 5, "INHALT ist leer oder nicht gefunden");
assert.equal(new Set(liste.map((e) => e.pfad)).size, liste.length, "INHALT nennt eine Datei doppelt");

// ── Ordner und Liste decken sich ────────────────────────────────────────
function dateien(ordner, vor = "") {
  return readdirSync(ordner).flatMap((n) => {
    const voll = join(ordner, n);
    return statSync(voll).isDirectory() ? dateien(voll, `${vor}${n}/`) : [`${vor}${n}`];
  });
}
const imOrdner = dateien(paket).filter((p) => p !== "INHALT").sort();
assert.deepEqual(imOrdner, liste.map((e) => e.pfad).sort(), "pi/paket/ und INHALT decken sich nicht");
console.log(`✓ ${liste.length} Dateien in INHALT, alle im Ordner und umgekehrt`);

// ── Inhalt der Dateien ──────────────────────────────────────────────────
for (const { modus, pfad } of liste) {
  const text = readFileSync(join(paket, pfad), "utf8");
  assert.ok(!text.includes("\r"), `${pfad}: Zeilenende CRLF (am Pi bricht das die Shell)`);
  assert.ok(text.endsWith("\n"), `${pfad}: letzte Zeile ohne Zeilenende`);
  if (modus === "755") assert.match(text, /^#!\//, `${pfad}: ausführbar, aber ohne Shebang`);
}
console.log("✓ Zeilenenden und Shebangs");

// ── setup-monitor.sh kopiert genau diese Liste ──────────────────────────
// Mit Ziel: Bis der Updater-Kern kommt (PR 3 der Spec), landen die Dateien an
// den Orten, an denen sie seit jeher liegen — bestehende Karten und die
// Anleitung verlassen sich darauf. Eine auskommentierte oder umgebogene Zeile
// fällt hier auf.
const ZIELE = {
  "bin/bts-cec.sh": "sudo install -m 755 \"$PAKET_DIR/bin/bts-cec.sh\" /usr/local/bin/bts-cec.sh",
  "units/bts-cec.service": "sudo install -m 644 \"$PAKET_DIR/units/bts-cec.service\" /etc/systemd/system/bts-cec.service",
  "share/no-url.html": "install -m 644 \"$PAKET_DIR/share/no-url.html\" \"$ASSET_DIR/no-url.html\"",
  "bin/bts-monitor.sh": "install -m 755 \"$PAKET_DIR/bin/bts-monitor.sh\" \"$HOME/.local/bin/bts-monitor.sh\"",
  xinitrc: "install -m 755 \"$PAKET_DIR/xinitrc\" \"$HOME/.xinitrc\"",
};
const setupZeilen = setup.split("\n").map((z) => z.trim());
for (const { modus, pfad } of liste) {
  const soll = ZIELE[pfad];
  assert.ok(soll, `${pfad}: neues Paket-Stück ohne Ziel — in ZIELE (dieser Test) und in setup-monitor.sh eintragen`);
  assert.ok(soll.includes(`install -m ${modus} `), `${pfad}: Rechte in INHALT (${modus}) und im Test weichen ab`);
  assert.ok(setupZeilen.includes(soll), `setup-monitor.sh kopiert ${pfad} nicht wie erwartet:\n  ${soll}`);
}
assert.match(setup, /^ASSET_DIR="\$HOME\/\.local\/share\/bts-monitor"$/m, "Ziel der Hinweisseite geändert");
const kopiert = [...setup.matchAll(/"\$PAKET_DIR\/([^"]+)"/g)].map((m) => m[1]).filter((p) => p !== "INHALT" && p !== "$datei");
for (const p of kopiert) {
  assert.ok(liste.some((e) => e.pfad === p), `setup-monitor.sh kopiert ${p}, das nicht in INHALT steht`);
}
// Die alten Heredocs dürfen nicht zurückkommen — sonst gibt es zwei Quellen.
for (const marke of ["<<'CEC'", "<<'UNIT'", "<<'HTML'", "<<XINIT", 'bts-monitor.sh" <<']) {
  assert.ok(!setup.includes(marke), `setup-monitor.sh enthält wieder einen eingebetteten Dateirumpf (${marke})`);
}
console.log("✓ setup-monitor.sh kopiert jede Datei der Liste und bettet keine mehr ein");

// ── Keine fest eingetragenen Werte im Kiosk-Skript ──────────────────────
const kiosk = readFileSync(join(paket, "bin/bts-monitor.sh"), "utf8");
assert.ok(!/^BROWSER="\//m.test(kiosk), "Kiosk-Skript: Browser-Pfad fest eingetragen");
assert.ok(!/^NO_URL_PAGE="file:\/\/\//m.test(kiosk), "Kiosk-Skript: Hinweisseite mit festem Pfad");
console.log("✓ Kiosk-Skript ohne fest eingetragene Pfade");

// ── Verhalten: Werte werden zur Laufzeit gefunden ───────────────────────
// Braucht bash und readlink wie am Pi; am Windows-Runner gibt es das nicht
// verlässlich, dort bleibt es bei den Prüfungen oben (der Linux-Job läuft
// denselben Test vollständig).
if (process.platform === "win32") {
  console.log("– Laufzeit-Werte übersprungen (Windows)");
} else {
  const tmp = realpathSync(mkdtempSync(join(tmpdir(), "bts-pi-quelle-")));
  try {
    const attrappen = (namen) => {
      const dir = mkdtempSync(join(tmp, "path-"));
      for (const n of namen) {
        writeFileSync(join(dir, n), "#!/bin/sh\nexit 0\n");
        chmodSync(join(dir, n), 0o755);
      }
      return dir;
    };
    // Eigener Werkzeug-Ordner statt /usr/bin im PATH: Auf einem Rechner mit
    // installiertem Chromium (CI-Runner) fände das Skript sonst den echten
    // Browser und nicht die Attrappe.
    const werkzeug = mkdtempSync(join(tmp, "werkzeug-"));
    for (const n of ["bash", "readlink", "dirname"]) {
      const echt = execFileSync("sh", ["-c", `command -v ${n}`], { encoding: "utf8" }).trim();
      symlinkSync(echt, join(werkzeug, n));
    }
    const werte = (skript, home, pfadDir) => {
      const aus = execFileSync(join(werkzeug, "bash"), [skript], {
        env: { PATH: `${pfadDir}${delimiter}${werkzeug}`, HOME: home, BTS_NUR_WERTE: "1" },
        encoding: "utf8",
      });
      return Object.fromEntries(aus.trim().split("\n").map((z) => z.split(/=(.*)/s).slice(0, 2)));
    };

    // browser_wird_zur_laufzeit_gefunden: nur „chromium" vorhanden (Trixie).
    const nurChromium = attrappen(["chromium"]);
    const home = join(tmp, "home");
    mkdirSync(join(home, ".local/bin"), { recursive: true });
    mkdirSync(join(home, ".local/share/bts-monitor"), { recursive: true });
    copyFileSync(join(paket, "bin/bts-monitor.sh"), join(home, ".local/bin/bts-monitor.sh"));
    copyFileSync(join(paket, "share/no-url.html"), join(home, ".local/share/bts-monitor/no-url.html"));
    let w = werte(join(home, ".local/bin/bts-monitor.sh"), home, nurChromium);
    assert.equal(w.BROWSER, join(nurChromium, "chromium"));
    // hinweisseite_am_alten_ort: so richtet setup-monitor.sh heute ein.
    assert.equal(w.NO_URL_PAGE, `file://${home}/.local/share/bts-monitor/no-url.html`);

    // Bookworm-Desktop nennt den Browser chromium-browser; der hat Vorrang.
    const beide = attrappen(["chromium", "chromium-browser"]);
    w = werte(join(home, ".local/bin/bts-monitor.sh"), home, beide);
    assert.equal(w.BROWSER, join(beide, "chromium-browser"));

    // hinweisseite_relativ_zum_skript: Layout des Pakets (bin/ neben share/),
    // aufgerufen über einen Verweis wie später /opt/bts/aktiv.
    const version = join(tmp, "pakete/7");
    mkdirSync(join(version, "bin"), { recursive: true });
    mkdirSync(join(version, "share"), { recursive: true });
    copyFileSync(join(paket, "bin/bts-monitor.sh"), join(version, "bin/bts-monitor.sh"));
    copyFileSync(join(paket, "share/no-url.html"), join(version, "share/no-url.html"));
    symlinkSync("pakete/7", join(tmp, "aktiv"));
    w = werte(join(tmp, "aktiv/bin/bts-monitor.sh"), home, nurChromium);
    assert.equal(w.NO_URL_PAGE, `file://${version}/share/no-url.html`);
    console.log("✓ Browser und Hinweisseite werden zur Laufzeit gefunden");
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
}
console.log("\nPi-Quelle geprüft.");
