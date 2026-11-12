// Nur-Lese-Modus am Pi: bei wenig Arbeitsspeicher bleibt die Karte beschreibbar.
//
// Auf dem Pi Zero 2 W (512 MB) blieb das Bild mit dem RAM-Overlay schwarz
// (Feldtest 12.11.2026). Der Dienst bts-nur-lesen.sh entscheidet deshalb nach
// dem Arbeitsspeicher. Dieser Test zieht den Entscheidungsblock aus
// pi/setup-monitor.sh und lässt ihn mit gespielten /proc/meminfo-Dateien laufen.
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import assert from "node:assert/strict";

const skript = readFileSync(new URL("../pi/setup-monitor.sh", import.meta.url), "utf8");
const start = skript.indexOf("ram_kb=$(awk");
const ende = skript.indexOf("\nif aktiv; then", start);
assert.ok(start > 0 && ende > start, "RAM-Block in setup-monitor.sh nicht gefunden");
const block = skript.slice(start, ende);

const dir = mkdtempSync(join(tmpdir(), "bts-ram-"));
function lauf(meminfo, soll = "nur-lesen") {
  const datei = join(dir, "meminfo");
  if (meminfo !== null) writeFileSync(datei, meminfo);
  const r = spawnSync(
    "bash",
    ["-c", `log() { echo "LOG: $*"; }\nsoll=${soll}\n${block}\necho "soll=$soll"`],
    { env: { ...process.env, BTS_MEMINFO: meminfo === null ? join(dir, "gibtsnicht") : datei }, encoding: "utf8" },
  );
  assert.equal(r.status, 0, r.stderr);
  return r.stdout;
}
const info = (kb) => `MemTotal:       ${kb} kB\nMemFree:        100000 kB\n`;

try {
  // Pi Zero 2 W meldet etwa 415–480 MB.
  assert.match(lauf(info(415000)), /soll=schreiben/, "Zero 2 W muss beschreibbar bleiben");
  assert.match(lauf(info(415000)), /LOG: .*415000 kB/, "Zero 2 W: Grund steht im Log");
  // Pi 3 B+ hat 1 GB (≈ 900–950 MB sichtbar), Pi 4/5 mehr: Schreibschutz bleibt.
  assert.match(lauf(info(948000)), /soll=nur-lesen/, "Pi 3 B+ behält den Schreibschutz");
  assert.match(lauf(info(3900000)), /soll=nur-lesen/, "Pi 4 behält den Schreibschutz");
  // Ist die Karte ohnehin beschreibbar gewünscht, bleibt es dabei – ohne Log.
  const schon = lauf(info(415000), "schreiben");
  assert.match(schon, /soll=schreiben/);
  assert.doesNotMatch(schon, /LOG:/, "kein Log, wenn nichts zu entscheiden ist");
  // Unlesbare oder fehlende Angabe darf den Schutz nicht aushebeln.
  assert.match(lauf("Quatsch\n"), /soll=nur-lesen/, "unlesbare meminfo: Schutz bleibt");
  assert.match(lauf(null), /soll=nur-lesen/, "fehlende meminfo: Schutz bleibt");
  console.log("✓ Nur-Lese-Modus nach Arbeitsspeicher");
} finally {
  rmSync(dir, { recursive: true, force: true });
}
