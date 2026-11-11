/**
 * Schlankes Undo-Format des Zähl-Tablets (Spec tablet-stand-robust,
 * ADR 0084). Prüft die kanonische Fassung aus `src/io/undoStand.mjs`; die
 * Inline-Kopie in `tablet.html` vergleicht `scripts/test-inline-kopien.mjs`.
 */
import assert from "node:assert/strict";
import {
  SPIEGEL_BUDGET,
  verlaufSchnitt,
  verlaufAufSchnitt,
  verlaufAusStand,
  verlaufIstPraefix,
  historyNormalisieren,
  spiegelKuerzen,
  utf8Laenge,
  verlaufGueltig,
} from "../src/io/undoStand.mjs";
import { mannschaftenGetauscht } from "../src/io/seitenKorrektur.mjs";

const kopie = (x) => JSON.parse(JSON.stringify(x));
const log = (...saetze) => ({
  sets: saetze.map((p) => ({ startA: 0, startB: 0, points: p })),
  midGame: false,
});

// ── Schnitt und Wiederherstellung ─────────────────────────────────────
assert.equal(verlaufSchnitt(null), null, "kein Verlauf → kein Schnitt");
assert.deepEqual(verlaufSchnitt(log("AAB", "B")), [[3, 1], false]);

{
  // Über eine Satzgrenze zurück: Satz 2 verschwindet, Satz 1 wird gekürzt.
  const frueher = log("AABAB");
  const jetzt = log("AABABAA", "BA");
  assert.deepEqual(verlaufAufSchnitt(jetzt, verlaufSchnitt(frueher)), frueher);
  assert.deepEqual(jetzt, log("AABABAA", "BA"), "Eingabe bleibt unberührt");
}
{
  // Der Startstand eines Satzes (Mid-Game-Einstieg) bleibt erhalten, das
  // midGame-Flag kommt aus dem Schnitt, nicht aus dem aktuellen Verlauf.
  const frueher = { sets: [{ startA: 21, startB: 15, points: "" }, { startA: 3, startB: 2, points: "A" }], midGame: true };
  const jetzt = kopie(frueher);
  jetzt.sets[1].points = "AAB";
  assert.deepEqual(verlaufAufSchnitt(jetzt, verlaufSchnitt(frueher)), frueher);
  const ohne = log("A");
  const mit = { ...log("AB"), midGame: true };
  assert.equal(verlaufAufSchnitt(mit, verlaufSchnitt(ohne)).midGame, false);
}
// Passt der Schnitt nicht (Verlauf kürzer als verlangt) → ehrlich null.
assert.equal(verlaufAufSchnitt(log("A"), [[3], false]), null);
assert.equal(verlaufAufSchnitt(log("AAA"), [[3, 1], false]), null);
assert.equal(verlaufAufSchnitt(null, [[1], false]), null);
assert.equal(verlaufAufSchnitt(log("A"), null), null);

// ── verlaufAusStand: Vollkopie vor Schnitt vor nichts ─────────────────
{
  const voll = log("AB");
  const aus = verlaufAusStand({ rallyLog: voll, rl: [[1], false] }, log("ABAB"));
  assert.deepEqual(aus, voll);
  assert.notEqual(aus, voll, "Vollkopie wird kopiert, nicht geteilt");
  assert.deepEqual(verlaufAusStand({ rl: [[1], false] }, log("ABAB")), log("A"));
  assert.equal(verlaufAusStand({ rl: null }, log("ABAB")), null);
  assert.equal(verlaufAusStand({}, log("ABAB")), null, "uralter Stand ohne Feld");
}

// ── Ein ganzes Spiel: 50 Undo-Stände, jeder Schritt zurück stimmt ─────
function spiele(punkte, mitKorrekturBei) {
  // Minimaler Tablet-Ablauf: vor jedem Punkt ein Undo-Stand im Format 2.
  let z = { rallyLog: null, history: [], erwartet: [] };
  const einzel = { matchId: 1, teamA: [{ id: 1 }], teamB: [{ id: 11 }] };
  let seiten = { match: einzel, teamOnSide: { a: "left", b: "right" }, positions: null, serveSetupTeam: null };
  for (let n = 0; n < punkte.length; n++) {
    if (n === mitKorrekturBei) {
      // „Mannschaften tauschen": eigener Undo-Stand MIT Vollkopie.
      z.erwartet.push(kopie(z.rallyLog));
      z.history.push({ rallyLog: kopie(z.rallyLog), korrektur: { art: "ab" } });
      const t = mannschaftenGetauscht({ ...seiten, rallyLog: z.rallyLog });
      z.rallyLog = t.rallyLog;
      seiten = { ...seiten, teamOnSide: t.teamOnSide };
    }
    z.erwartet.push(kopie(z.rallyLog));
    z.history.push({ rl: verlaufSchnitt(z.rallyLog) });
    if (z.history.length > 50) { z.history.shift(); z.erwartet.shift(); }
    const [satz, w] = punkte[n];
    if (!z.rallyLog) z.rallyLog = { sets: [], midGame: false };
    while (z.rallyLog.sets.length <= satz) z.rallyLog.sets.push({ startA: 0, startB: 0, points: "" });
    z.rallyLog.sets[satz].points += w;
  }
  return z;
}
function zurueckBisLeer(z, name) {
  let aktuell = z.rallyLog;
  while (z.history.length) {
    aktuell = verlaufAusStand(z.history.pop(), aktuell);
    assert.deepEqual(aktuell, z.erwartet.pop(), `${name}: Schritt ${z.history.length}`);
  }
}
const ZUG = [];
for (let s = 0; s < 3; s++) for (let n = 0; n < 40; n++) ZUG.push([s, n % 3 ? "A" : "B"]);
zurueckBisLeer(spiele(ZUG.slice(0, 30)), "kurzes Spiel bis 0:0");
zurueckBisLeer(spiele(ZUG), "drei Sätze, Kappung bei 50");
zurueckBisLeer(spiele(ZUG, 100), "Korrektur im Verlauf (ADR 0067)");
zurueckBisLeer(spiele(ZUG, 119), "Korrektur als oberster Schritt");
assert.equal(spiele(ZUG).history.length, 50);

// ── Normalisieren: alter Stand nach Update mitten im Spiel ────────────
{
  // Altes Format: jeder Undo-Stand mit Vollkopie von Verlauf und Ereignissen.
  const neu = spiele(ZUG, 100);
  const alt = kopie(neu);
  let aktuell = alt.rallyLog;
  const volle = [];
  for (let i = alt.history.length - 1; i >= 0; i--) {
    aktuell = verlaufAusStand(alt.history[i], aktuell);
    volle.unshift({ rallyLog: kopie(aktuell), events: [{ id: "x" }], eventSeq: 3, korrektur: alt.history[i].korrektur });
  }
  const norm = historyNormalisieren(volle, alt.rallyLog);
  assert.equal(norm.length, volle.length);
  assert.ok(norm.every((s) => !("events" in s)), "Ereignis-Kopien fallen weg");
  assert.ok(norm.every((s) => s.eventSeq === 3), "eventSeq bleibt");
  const vollkopien = norm.filter((s) => s.rallyLog);
  assert.equal(vollkopien.length, 1, "nur der Korrektur-Schritt behält die Vollkopie");
  assert.equal(vollkopien[0].korrektur.art, "ab");
  assert.ok("events" in volle[0], "Eingabe bleibt unberührt");
  zurueckBisLeer({ rallyLog: alt.rallyLog, history: norm, erwartet: kopie(neu.erwartet) }, "normalisierter Altstand");
  assert.ok(JSON.stringify(norm).length < JSON.stringify(volle).length / 3, "deutlich kleiner");
  // Zweimal normalisieren ändert nichts mehr (jedes Laden läuft hier durch).
  assert.deepEqual(historyNormalisieren(norm, alt.rallyLog), norm);
}
{
  // Vollkopie, die KEIN Präfix ist (fremd/beschädigt) → bleibt Vollkopie.
  const fremd = [{ rallyLog: log("BBB") }, { rallyLog: log("AA") }];
  const norm = historyNormalisieren(fremd, log("AAB"));
  assert.deepEqual(norm[1], { rl: [[2], false] });
  assert.deepEqual(norm[0], { rallyLog: log("BBB") });
  // Uralter Stand ohne Verlaufsfeld und ausdrückliches null → rl: null.
  assert.deepEqual(historyNormalisieren([{ a: 1 }, { rallyLog: null }], log("A")), [{ a: 1, rl: null }, { rl: null }]);
  assert.deepEqual(historyNormalisieren([], null), []);
}
{
  // Verlauf ging verloren (aktuell null), Undo-Stände tragen Vollkopien:
  // nichts zum Schnitt machen, was sich nicht belegen lässt.
  const norm = historyNormalisieren([{ rallyLog: log("A") }, { rallyLog: log("AB") }], null);
  assert.deepEqual(norm[1], { rallyLog: log("AB") });
  assert.deepEqual(norm[0], { rl: [[1], false] });
}

// ── Neuer Stand auf alter Seite (Übernahme, Rollback) ─────────────────
{
  // So stellte die Seite vor Format 2 den Verlauf wieder her:
  const alteSeite = (snap) => (snap.rallyLog ? kopie(snap.rallyLog) : null);
  // Punkte stimmen (stehen nicht im Verlauf); der Verlauf wird verworfen —
  // hingenommen und dokumentiert (ADR 0084). Der Korrektur-Schritt bleibt
  // auch dort verlustfrei.
  assert.equal(alteSeite({ rl: [[3], false] }), null);
  assert.deepEqual(alteSeite({ rallyLog: log("AB"), korrektur: { art: "ab" } }), log("AB"));
}

// ── Spiegel kürzen ────────────────────────────────────────────────────
{
  const klein = { a: 1, history: [{ x: 1 }, { x: 2 }] };
  const j = JSON.stringify(klein);
  assert.equal(spiegelKuerzen(klein, j, SPIEGEL_BUDGET), j, "unter Budget unverändert");

  const gross = { match: { id: 7 }, history: [] };
  for (let i = 0; i < 50; i++) gross.history.push({ nr: i, fuell: "x".repeat(2000) });
  const voll = JSON.stringify(gross);
  assert.ok(voll.length > SPIEGEL_BUDGET);
  const kurz = spiegelKuerzen(gross, voll, SPIEGEL_BUDGET);
  assert.ok(kurz.length <= SPIEGEL_BUDGET, `gekürzt passt (${kurz.length})`);
  const p = JSON.parse(kurz);
  assert.deepEqual(p.match, { id: 7 }, "alles außer history bleibt");
  assert.ok(p.history.length > 0 && p.history.length < 50);
  assert.equal(p.history[p.history.length - 1].nr, 49, "die JÜNGSTEN Stände bleiben");
  assert.equal(p.history[0].nr, 50 - p.history.length, "lückenlos von oben");
  assert.equal(gross.history.length, 50, "der lokale Stand bleibt vollständig");
  // So knapp wie nötig: ein Stand mehr hätte nicht gepasst.
  const einerMehr = JSON.stringify({ ...gross, history: gross.history.slice(50 - p.history.length - 1) });
  assert.ok(einerMehr.length > SPIEGEL_BUDGET);

  // Selbst ohne Undo-Stände zu groß → leere history, Rest unverändert
  // (Host/Relay verwerfen dann mit Log — nicht still).
  const riesig = { blob: "y".repeat(SPIEGEL_BUDGET + 10), history: [{ x: 1 }] };
  assert.deepEqual(JSON.parse(spiegelKuerzen(riesig, JSON.stringify(riesig), SPIEGEL_BUDGET)).history, []);
  assert.doesNotThrow(() => spiegelKuerzen({ a: "z".repeat(100) }, "z".repeat(100), 10));
}

// ── Bytes, nicht Zeichen (Host und Relay messen Bytes) ────────────────
{
  for (const t of ["", "abc", "Müller", "Größe ß", "李娜", "😀x", "a\u0000b"]) {
    assert.equal(utf8Laenge(t), Buffer.byteLength(t), `utf8Laenge(${JSON.stringify(t)})`);
  }
  // 30 000 CJK-Zeichen: weit unter 56 000 Zeichen, aber 90 000 Bytes.
  const cjk = { name: "李".repeat(30000), history: [{ x: 1 }, { x: 2 }] };
  const j = JSON.stringify(cjk);
  assert.ok(j.length < SPIEGEL_BUDGET && Buffer.byteLength(j) > 64 * 1024);
  assert.notEqual(spiegelKuerzen(cjk, j, SPIEGEL_BUDGET), j, "über dem Byte-Budget wird gekürzt");
  const viele = { history: Array.from({ length: 50 }, (_, i) => ({ i, t: "ü".repeat(1000) })) };
  const kurz = spiegelKuerzen(viele, JSON.stringify(viele), SPIEGEL_BUDGET);
  assert.ok(Buffer.byteLength(kurz) <= SPIEGEL_BUDGET, "gekürzter Spiegel passt in BYTES");
  assert.ok(JSON.parse(kurz).history.length > 0);
}

// ── Fremder oder beschädigter Zustand wirft nie (Übernahme, state_restore) ──
{
  const KAPUTT = [
    null, undefined, 7, "x", [], {}, { sets: null }, { sets: "AB" }, { sets: [null] }, { sets: [{}] },
    { sets: [{ points: 5 }] }, { sets: [{ points: "A" }, 3] },
  ];
  for (const k of KAPUTT) {
    assert.equal(verlaufGueltig(k), false, `ungültig: ${JSON.stringify(k)}`);
    assert.equal(verlaufSchnitt(k), null);
    assert.equal(verlaufAufSchnitt(k, [[1], false]), null);
    assert.equal(verlaufAusStand({ rl: [[1], false] }, k), null);
    assert.equal(verlaufAusStand({ rallyLog: k }, log("AB")), null);
    assert.doesNotThrow(() => historyNormalisieren([{ rl: [[1], false] }, { rallyLog: k }, { rallyLog: log("A") }], k));
    assert.doesNotThrow(() => verlaufIstPraefix(log("A"), k));
  }
  assert.equal(verlaufGueltig(log()), true);
  assert.equal(verlaufGueltig(log("A", "")), true);
  // Unsinnige Schnitte: nie ein falscher Verlauf, immer null.
  for (const rl of [7, "x", {}, [], [7], [[-1], false], [[1.5], false], [["2"], false], [[NaN], false], [[1e9], false]]) {
    assert.equal(verlaufAufSchnitt(log("AB"), rl), null, `Schnitt ${JSON.stringify(rl)}`);
  }
  // Keine Liste / Einträge, die keine Undo-Stände sind → fallen weg.
  assert.deepEqual(historyNormalisieren(null, null), []);
  assert.deepEqual(historyNormalisieren("abc", null), []);
  assert.deepEqual(historyNormalisieren([null, 3, "x", [1], { a: 1 }], null), [{ a: 1, rl: null }]);
  // `__proto__` aus JSON wird nicht zum Prototyp der Kopie.
  const boese = JSON.parse('[{"a":1,"__proto__":{"rl":[[9],false],"korrektur":{"art":"ab"}}}]');
  const [sauber] = historyNormalisieren(boese, log("AB"));
  assert.equal(Object.getPrototypeOf(sauber), Object.prototype);
  assert.deepEqual(sauber, { a: 1, rl: null });
  assert.equal({}.rl, undefined, "nichts Globales verändert");
}

// ── Mid-Game-Einstieg und Tausch → Reload → zurück (Review 11.11.2026) ─
{
  // Einstieg nach Satz 1 bei 5:3: Füller-Satz ohne Punkte, Startstand ≠ 0.
  const start = { sets: [{ startA: 21, startB: 15, points: "" }, { startA: 5, startB: 3, points: "" }], midGame: true };
  let cur = kopie(start);
  const hist = [], soll = [];
  for (const w of "ABBA") {
    soll.push(kopie(cur));
    hist.push({ rl: verlaufSchnitt(cur) });
    cur.sets[1].points += w;
  }
  // „Reload": durch JSON und die Normalisierung.
  let h = historyNormalisieren(JSON.parse(JSON.stringify(hist)), cur);
  while (h.length) {
    cur = verlaufAusStand(h.pop(), cur);
    assert.deepEqual(cur, soll.pop(), "Mid-Game: Schritt zurück nach Reload");
  }
  assert.deepEqual(cur, start, "Startstände der Sätze bleiben erhalten");

  // Tausch mitten im Spiel, weiterzählen, Reload, alles zurück.
  const z = spiele(ZUG.slice(0, 40), 20);
  const nachReload = historyNormalisieren(JSON.parse(JSON.stringify(z.history)), z.rallyLog);
  assert.equal(nachReload.filter((s) => s.rallyLog).length, 1, "Korrektur behält die Vollkopie über den Reload");
  zurueckBisLeer({ rallyLog: z.rallyLog, history: nachReload, erwartet: z.erwartet }, "Tausch → Reload → zurück");
}

console.log("✓ undoStand: alle Prüfungen bestanden");
