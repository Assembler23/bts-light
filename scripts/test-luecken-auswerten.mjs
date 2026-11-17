/**
 * Feldtest-Auswertung „Hotfix ohne Neustart": scripts/luecken-auswerten.mjs.
 *
 * Warum: Das Skript entscheidet, ob Erfolgskriterium 2 der Spec als erfüllt
 * ins Protokoll kommt. Ordnet es eine Installer-Lücke dem Tausch zu oder
 * meldet es „erfüllt" nach drei Läufen, steht eine falsche Zahl in der Spec.
 * Die Muster hängen am Wortlaut der Logzeilen im Rust-Kern — auch das wird
 * geprüft.
 */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { KRITERIUM, auswerten, bericht, lueckenAus, median } from "./luecken-auswerten.mjs";

const z = (zeit, text) => `2026-11-17T${zeit}Z  INFO bts_light_lib::commands: ${text}`;
const luecke = (zeit, ms) =>
  z(zeit, `Wiederanlauf nach Update: Tablet-Port wieder offen, Lücke ${ms} ms (Anhalten der Vorversion → Port offen)`);

// ── Zuordnung zum Weg ──────────────────────────────────────────────────────
const ankunft = (zeit, v) => z(zeit, `Kernwechsel: Ankunft von v${v} gemeldet`);
const log = [
  z("10:00:00.000001", "Kernwechsel auf v0.9.442: Programmdatei getauscht"),
  z("10:00:01.000001", "Kernwechsel auf v0.9.442: Wächter gestartet (Prozess 4711), App endet"),
  // Die Ankunft kommt mal nach der Lücken-Zeile …
  luecke("10:00:03.500000", 2500),
  ankunft("10:00:03.900000", "0.9.442"),
  z("11:00:00.000001", "Rückweg auf v0.9.441: Programmdatei getauscht"),
  z("11:00:00.500001", "Kernwechsel auf v0.9.441: Wächter gestartet (Prozess 4712), App endet"),
  // … mal davor.
  ankunft("11:00:02.800000", "0.9.441"),
  luecke("11:00:03.000000", 2400),
  z("12:00:00.000001", "Update auf v0.9.442 wird eingebaut (Übertragung lief: true)"),
  luecke("12:00:21.000000", 20800),
  // Eine Lücke ohne Ankündigung davor (Log abgeschnitten).
  luecke("13:00:00.000000", 9000),
  "kaputte Zeile ohne Zeitstempel: Lücke 1 ms",
];
// Reihenfolge der Zeilen ist egal (mehrere Dateien, gedreht).
let l = lueckenAus([...log].reverse());
assert.deepEqual(
  l.map((x) => [x.weg, x.version, x.ms]),
  [
    ["wechsel", "0.9.442", 2500],
    ["rueckweg", "0.9.441", 2400],
    ["installer", "0.9.442", 20800],
    ["unbekannt", null, 9000],
  ],
);
assert.equal(l[0].zeit, "2026-11-17T10:00:03.500000Z");

// Ein Abgang erklärt genau eine Lücke — eine zweite danach gehört ihm nicht.
l = lueckenAus([
  z("10:00:01", "Kernwechsel auf v1.0.1: Wächter gestartet (Prozess 1), App endet"),
  luecke("10:00:03", 2000),
  ankunft("10:00:04", "1.0.1"),
  luecke("10:30:00", 7000),
]);
assert.deepEqual(l.map((x) => x.weg), ["wechsel", "unbekannt"]);

// Ein abgebrochener Rückweg (getauscht, aber kein Wächter) färbt den nächsten
// Wechsel auf eine ANDERE Version nicht um.
l = lueckenAus([
  z("09:00:00", "Rückweg auf v1.0.0: Programmdatei getauscht"),
  z("10:00:01", "Kernwechsel auf v1.0.2: Wächter gestartet (Prozess 1), App endet"),
  luecke("10:00:03", 2000),
  ankunft("10:00:04", "1.0.2"),
]);
assert.deepEqual(l.map((x) => x.weg), ["wechsel"]);

// Rückfall: Der Wächter hat zurückgerollt, die ALTE Version startet wieder und
// loggt die Lücke — eine Ankunft der Zielversion gibt es nicht. Das ist kein
// Tausch-Lauf, auch wenn die Lücke schön kurz ist.
l = lueckenAus([
  z("10:00:01", "Kernwechsel auf v1.0.2: Wächter gestartet (Prozess 1), App endet"),
  luecke("10:00:02", 1100),
  // Der nächste Versuch gelingt; seine Ankunft gehört nicht dem ersten.
  z("10:10:01", "Kernwechsel auf v1.0.2: Wächter gestartet (Prozess 2), App endet"),
  luecke("10:10:03", 2300),
  ankunft("10:10:04", "1.0.2"),
  // Am Ende des Logs: Abgang und Lücke, aber (noch) keine Ankunft.
  z("10:20:01", "Kernwechsel auf v1.0.3: Wächter gestartet (Prozess 3), App endet"),
  luecke("10:20:02", 900),
]);
assert.deepEqual(l.map((x) => [x.weg, x.ms]), [
  ["rueckfall", 1100],
  ["wechsel", 2300],
  ["rueckfall", 900],
]);
// Die Ankunft einer ANDEREN Version zählt nicht.
l = lueckenAus([
  z("10:00:01", "Kernwechsel auf v1.0.2: Wächter gestartet (Prozess 1), App endet"),
  luecke("10:00:02", 1100),
  ankunft("10:00:03", "1.0.1"),
]);
assert.deepEqual(l.map((x) => x.weg), ["rueckfall"]);
let a0 = auswerten(l);
assert.equal(a0.tausch.laeufe, 0, "ein Rückfall ist kein Tausch-Lauf");
assert.equal(a0.rueckfall.laeufe, 1);

// --seit: Ältere Läufe (frühere Updates, „nicht im selben Verfahren") entfallen.
l = lueckenAus(log, { seit: "2026-11-17T11:30" });
assert.deepEqual(l.map((x) => x.weg), ["installer", "unbekannt"]);
assert.equal(lueckenAus(log, { seit: "2026-11-18" }).length, 0);

// ── Die Muster stimmen mit dem Wortlaut im Rust-Kern überein ───────────────
const quelle = (datei) => readFileSync(new URL(`../src-tauri/src/${datei}`, import.meta.url), "utf8");
const commands = quelle("commands.rs");
assert.ok(quelle("tablet/state.rs").includes("Wiederanlauf nach Update: Tablet-Port wieder offen, Lücke {ms} ms"));
assert.ok(commands.includes("Kernwechsel auf v{nach}: Wächter gestartet"));
assert.ok(commands.includes("Rückweg auf v{version}: Programmdatei getauscht"));
assert.ok(commands.includes("Update auf v{} wird eingebaut"));
assert.ok(commands.includes("Kernwechsel: Ankunft von v{eigene} gemeldet"));

// ── Median ─────────────────────────────────────────────────────────────────
assert.equal(median([]), null);
assert.equal(median([5]), 5);
assert.equal(median([3, 1, 2]), 2);
assert.equal(median([4, 1, 3, 2]), 3, "gerade Anzahl: Mittel der beiden mittleren, gerundet");

// ── Urteil ─────────────────────────────────────────────────────────────────
const laeufe = (weg, werte) => werte.map((ms, i) => ({ zeit: `t${i}`, weg, version: "1", ms }));
const zehn = (ms) => Array(KRITERIUM.laeufe).fill(ms);

// Zu wenige Läufe: nie „erfüllt", auch wenn die Zahlen gut aussehen.
let a = auswerten(laeufe("wechsel", [2000, 2100, 2200]));
assert.equal(a.urteil, "offen");
assert.deepEqual(a.tausch, { laeufe: 3, medianMs: 2100, maxMs: 2200 });
assert.equal(a.offen.length, 2, "Tausch- und Installer-Läufe fehlen");

// Je zehn Läufe, alles im Rahmen. Wechsel und Rückweg zählen gemeinsam.
a = auswerten([
  ...laeufe("wechsel", [2000, 2100, 2200, 2300, 2400]),
  ...laeufe("rueckweg", [2500, 2600, 2700, 2800, 2900]),
  ...laeufe("installer", zehn(20000)),
]);
assert.equal(a.urteil, "erfuellt");
assert.equal(a.tausch.laeufe, 10);
assert.equal(a.tausch.medianMs, 2450);
assert.deepEqual([a.offen, a.verletzt], [[], []]);

// Verfehlt schlägt offen: Ein Ausreißer über 5 s zählt sofort.
a = auswerten(laeufe("wechsel", [2000, 2000, 5001]));
assert.equal(a.urteil, "verfehlt");
assert.match(a.verletzt[0], /Maximum 5001 ms/);

// Median über 3 s.
a = auswerten([...laeufe("wechsel", zehn(3001)), ...laeufe("installer", zehn(20000))]);
assert.equal(a.urteil, "verfehlt");
assert.match(a.verletzt[0], /Median 3001 ms/);

// Schnell, aber nicht schnell genug gegenüber dem Installer (Drittel-Regel).
a = auswerten([...laeufe("wechsel", zehn(2500)), ...laeufe("installer", zehn(6000))]);
assert.equal(a.urteil, "verfehlt");
assert.match(a.verletzt[0], /einem Drittel des Installer-Wegs \(2000 ms\)/);

// Lücken ohne erkennbaren Weg zählen für nichts.
a = auswerten(laeufe("unbekannt", zehn(100)));
assert.equal(a.tausch.laeufe, 0);
assert.equal(a.unbekannt.laeufe, 10);
assert.equal(a.urteil, "offen");
// … verhindern aber „erfüllt": Eine 30-s-Lücke unklarer Herkunft könnte ein
// Tausch gewesen sein.
a = auswerten([
  ...laeufe("wechsel", zehn(2000)),
  ...laeufe("installer", zehn(20000)),
  ...laeufe("unbekannt", [30000]),
]);
assert.equal(a.urteil, "offen");
assert.match(a.offen[0], /1 Lücke\(n\) ohne erkennbaren Weg/);
// Rückfälle stören das Urteil nicht, zählen aber auch nicht mit.
a = auswerten([
  ...laeufe("wechsel", zehn(2000)),
  ...laeufe("installer", zehn(20000)),
  ...laeufe("rueckfall", [900, 16000]),
]);
assert.equal(a.urteil, "erfuellt");
assert.equal(a.tausch.laeufe, 10);

// ── Bericht ────────────────────────────────────────────────────────────────
const text = bericht(lueckenAus(log));
assert.match(text, /\| 2026-11-17 10:00:03 \| Wechsel \| 0\.9\.442 \| 2500 ms \|/);
assert.match(text, /\| 2026-11-17 11:00:03 \| Rückweg \| 0\.9\.441 \| 2400 ms \|/);
assert.match(text, /Tausch \(Wechsel \+ Rückweg\): 2 Läufe, Median 2450 ms, Maximum 2500 ms/);
assert.match(text, /Installer: 1 Läufe, Median 20800 ms/);
assert.match(text, /Ohne erkennbaren Weg: 1 Läufe/);
assert.match(text, /Kriterium 2 noch OFFEN/);
assert.match(
  bericht(lueckenAus([
    z("10:00:01", "Kernwechsel auf v1.0.2: Wächter gestartet (Prozess 1), App endet"),
    luecke("10:00:02", 1100),
  ])),
  /\| Rückfall \(zählt nicht\) \| 1\.0\.2 \| 1100 ms \|[\s\S]*Rückfälle \(nicht gewertet\): 1 Läufe/,
);
assert.match(text, /noch 8 Tausch-Läufe bis 10/);
assert.match(bericht([]), /Tausch \(Wechsel \+ Rückweg\): 0 Läufe\n/);

console.log("OK — Lücken-Auswertung: Zuordnung, Median, Urteil und Bericht geprüft.");
