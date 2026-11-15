/**
 * Autovervollständigung im Ergebnis-Dialog des Zähl-Tablets
 * (`src/io/satzErgaenzen.mjs`). Ein falscher Vorschlag wäre schlimmer als
 * keiner: Er sähe wie eine Eingabe aus und ginge als Ergebnis nach BTP.
 */
import assert from "node:assert/strict";
import { satzErgaenzen, gegenfeldNach } from "../src/io/satzErgaenzen.mjs";

// ── Sieger-Punkte zu Verlierer-Punkten ────────────────────────────────
// Bis 21, Deckel 30.
assert.equal(satzErgaenzen(0, 21, 30), 21, "21:0");
assert.equal(satzErgaenzen(11, 21, 30), 21, "11 → 21");
assert.equal(satzErgaenzen(19, 21, 30), 21, "19 → 21 (letzter Stand ohne Verlängerung)");
assert.equal(satzErgaenzen(20, 21, 30), 22, "20 → 22 (Verlängerung)");
for (const p of [21, 22, 28, 29, 30, 31, 99]) {
  assert.equal(satzErgaenzen(p, 21, 30), null, `${p}: ab dem Ziel nicht eindeutig`);
}
// Bis 15 (Deckel 21) und bis 11 (Deckel 15).
assert.equal(satzErgaenzen(9, 15, 21), 15);
assert.equal(satzErgaenzen(14, 15, 21), 16);
assert.equal(satzErgaenzen(15, 15, 21), null);
assert.equal(satzErgaenzen(5, 11, 15), 11);
assert.equal(satzErgaenzen(10, 11, 15), 12);
// Ohne Verlängerung (Deckel = Ziel): 20 heißt 21:20.
assert.equal(satzErgaenzen(20, 21, 21), 21, "Deckel am Ziel");
// Deckel unbrauchbar → zwei Punkte Abstand.
for (const cap of [undefined, null, 0, 5, NaN, "30"]) {
  assert.equal(satzErgaenzen(20, 21, cap), 22, `Deckel ${cap}`);
}
// Zeitspiel und Unsinn: nie ein Vorschlag.
assert.equal(satzErgaenzen(11, 99, 104), null, "Zeitspiel");
assert.equal(satzErgaenzen(11, 0, 30), null);
assert.equal(satzErgaenzen(11, undefined, 30), null);
for (const p of [-1, 1.5, NaN, "11", null, undefined]) {
  assert.equal(satzErgaenzen(p, 21, 30), null, `Punkte ${p}`);
}
// Jeder Vorschlag ist ein regulär beendeter Satz.
for (const [target, cap] of [[21, 30], [15, 21], [11, 15], [21, 21]]) {
  for (let p = 0; p < target; p++) {
    const s = satzErgaenzen(p, target, cap);
    assert.ok(s >= target && s <= cap && (s - p >= 2 || s === cap), `${p}:${s} bis ${target}/${cap}`);
  }
}

// ── Das Gegenfeld ─────────────────────────────────────────────────────
// Unberührte 0: der Vorschlag kommt.
assert.equal(gegenfeldNach("11", 0, false, 21, 30), 21);
assert.equal(gegenfeldNach("20", 0, false, 21, 30), 22);
// Der Vorschlag folgt der Eingabe: „1" → 21, „11" → 21, „2" → 21, „20" → 22.
assert.equal(gegenfeldNach("1", 0, false, 21, 30), 21);
assert.equal(gegenfeldNach("11", 21, true, 21, 30), 21);
assert.equal(gegenfeldNach("20", 21, true, 21, 30), 22);
// … und geht wieder, wenn die Eingabe keinen mehr hergibt („2" → „21").
assert.equal(gegenfeldNach("21", 21, true, 21, 30), 0, "Vorschlag zurücknehmen");
assert.equal(gegenfeldNach("", 21, true, 21, 30), 0, "Feld geleert");
assert.equal(gegenfeldNach("  ", 21, true, 21, 30), 0);
// Von Hand gesetzt: nie anfassen.
assert.equal(gegenfeldNach("11", 19, false, 21, 30), null);
assert.equal(gegenfeldNach("20", 21, false, 21, 30), null, "auch wenn es nicht passt");
assert.equal(gegenfeldNach("", 21, false, 21, 30), null);
// Kein Vorschlag, kein früherer Vorschlag: Finger weg.
assert.equal(gegenfeldNach("21", 0, false, 21, 30), null);
assert.equal(gegenfeldNach("", 0, false, 21, 30), null);
assert.equal(gegenfeldNach(null, 0, false, 21, 30), null);
assert.equal(gegenfeldNach("abc", 0, false, 21, 30), null);
assert.equal(gegenfeldNach("-3", 0, false, 21, 30), null);
assert.equal(gegenfeldNach("1.5", 0, false, 21, 30), null);
// Zeitspiel: nie.
assert.equal(gegenfeldNach("11", 0, false, 99, 104), null);

console.log("OK: Satz ergänzen");
