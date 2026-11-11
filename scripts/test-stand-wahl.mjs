/**
 * Wessen Spielstand gilt? Regeln der Spec tablet-stand-robust, Abschnitt A
 * (ADR 0083) — am Modul `src/io/standWahl.mjs`. Der echte Seiten-Code wird
 * im zweiten Teil gegen dieselben Lagen gefahren.
 */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import * as w from "../src/io/standWahl.mjs";

// ── Fortschritt ───────────────────────────────────────────────────────
const stand = (saetze, lauf) => ({
  setsCompleted: saetze.map(([l, r]) => ({ left: l, right: r })),
  currentSet: { left: lauf[0], right: lauf[1] },
});
assert.deepEqual(w.fortschritt(stand([], [0, 0])), [0, 0]);
assert.deepEqual(w.fortschritt(stand([[21, 10]], [5, 3])), [1, 39]);
assert.deepEqual(w.fortschritt(stand([[10, 21]], [3, 5])), [1, 39], "seitenunabhängig");
assert.deepEqual(w.fortschritt(null), [0, 0]);
assert.deepEqual(w.fortschritt({ setsCompleted: [null, { left: "x" }], currentSet: 7 }), [2, 0], "Unlesbares zählt 0 Punkte");
// `finished` zählt nicht: ein beendeter Stand ist nicht „weiter" als derselbe offene.
assert.deepEqual(w.fortschritt({ ...stand([[21, 10], [21, 12]], [0, 0]), finished: true }), w.fortschritt(stand([[21, 10], [21, 12]], [0, 0])));
assert.equal(w.vergleiche([1, 39], [1, 39]), 0);
assert.equal(w.vergleiche([1, 5], [0, 40]), 1, "Sätze vor Punkten");
assert.equal(w.vergleiche([1, 38], [1, 39]), -1);

// ── Host-Sätze lesen und teilen ───────────────────────────────────────
const S = (...p) => p.map(([a, b]) => ({ a, b }));
assert.deepEqual(w.hostSaetzeLesen(S([21, 10], [0, 0])), S([21, 10]), "abschließendes 0:0 fällt weg");
assert.deepEqual(w.hostSaetzeLesen(S([0, 0])), []);
assert.deepEqual(w.hostSaetzeLesen(undefined), []);
assert.deepEqual(w.hostSaetzeLesen([{ a: "21", b: 10 }]), [], "kaputte Liste gilt als leer");
assert.deepEqual(w.hostSaetzeLesen([{ a: -1, b: 3 }]), []);
assert.deepEqual(w.hostSaetzeLesen([{ a: 1.5, b: 3 }]), []);
assert.deepEqual(w.hostSaetzeLesen(Array(10).fill({ a: 1, b: 0 })), [], "zu viele Sätze");

let t = w.hostSaetzeTeilen(S([21, 10], [5, 3]), 21, 30, 3);
assert.deepEqual(t, { fertig: S([21, 10]), laufend: { a: 5, b: 3 }, entschieden: false, fortschritt: [1, 39] });
t = w.hostSaetzeTeilen(S([21, 10], [0, 0]), 21, 30, 3);
assert.deepEqual(t.fertig, S([21, 10]));
assert.deepEqual(t.laufend, { a: 0, b: 0 }, "zwischen den Sätzen");
t = w.hostSaetzeTeilen(S([21, 10], [21, 15]), 21, 30, 3);
assert.equal(t.entschieden, true, "zwei gewonnene Sätze entscheiden best of 3");
assert.deepEqual(t.laufend, { a: 0, b: 0 });
assert.deepEqual(t.fortschritt, [2, 67]);
t = w.hostSaetzeTeilen(S([30, 29]), 21, 30, 3);
assert.equal(t.fertig.length, 1, "Deckel beendet den Satz");
t = w.hostSaetzeTeilen(S([21, 20]), 21, 30, 3);
assert.deepEqual(t.laufend, { a: 21, b: 20 }, "21:20 läuft noch");
t = w.hostSaetzeTeilen(S([15, 8], [3, 3]), 15, 0, 3);
assert.deepEqual(t.fertig, S([15, 8]), "ohne Deckel: Ableitung wie die Seite (15 → 21)");
t = w.hostSaetzeTeilen(S([40, 38]), 99, 0, 1);
assert.deepEqual(t.laufend, { a: 40, b: 38 }, "Zeit-Format endet nie von selbst");
t = w.hostSaetzeTeilen([], 21, 30, 3);
assert.deepEqual(t, { fertig: [], laufend: { a: 0, b: 0 }, entschieden: false, fortschritt: [0, 0] });

// ── Melde-Ring ────────────────────────────────────────────────────────
assert.equal(w.saetzeSchluessel(S([21, 10], [5, 3])), "21:10,5:3");
assert.equal(w.saetzeSchluessel(S([21, 10], [0, 0])), "21:10", "wie der Server das Geister-0:0 behandelt, ist egal");
let ring = null;
ring = w.ringMerken(ring, 7, S([1, 0]));
ring = w.ringMerken(ring, 7, S([1, 0]));
assert.deepEqual(ring, { matchId: 7, saetze: ["1:0"] }, "dieselbe Meldung zweimal: ein Eintrag");
for (let i = 2; i <= 12; i++) ring = w.ringMerken(ring, 7, S([i, 0]));
assert.equal(ring.saetze.length, w.MELDE_RING_MAX);
assert.equal(ring.saetze[0], "5:0", "die ältesten fallen heraus");
assert.equal(w.imRing(ring, 7, S([12, 0])), true);
assert.equal(w.imRing(ring, 7, S([12, 0], [0, 0])), true);
assert.equal(w.imRing(ring, 7, S([4, 0])), false, "aus dem Ring gefallen");
assert.equal(w.imRing(ring, 8, S([12, 0])), false, "anderes Spiel");
assert.equal(w.imRing(null, 7, S([12, 0])), false);
assert.deepEqual(w.ringMerken(ring, 8, S([1, 1])), { matchId: 8, saetze: ["1:1"] }, "neues Spiel beginnt den Ring neu");

// ── Stufe 1: state_restore ────────────────────────────────────────────
const F = (s, p) => [s, p];
const lokal = (matchId, rev, f) => ({ matchId, rev, fortschritt: f });
const kand = (matchId, rev, schreiber, f) => ({ matchId, rev, schreiber, fortschritt: f });
const r = (e) => w.wahlBeiRestore({ ownershipActive: true, authoritative: true, eigeneKennung: "A", ...e });

// Legacy: die alte Logik, egal was sonst gilt (AK10).
assert.equal(w.wahlBeiRestore({ ownershipActive: false, authoritative: true, lokal: null, kandidat: kand(7, 5, "B", F(1, 30)), eigeneKennung: "A" }), "legacy");
// 1. Übernahme/finalisiert.
assert.equal(r({ authoritative: false, lokal: lokal(7, 99, F(2, 80)), kandidat: kand(7, 1, "B", F(0, 1)) }), "adoptieren");
// 2. AK4: frisches Gerät an freiem Feld übernimmt den gesicherten Stand.
assert.equal(r({ lokal: null, kandidat: kand(7, 40, "B", F(1, 39)) }), "adoptieren");
assert.equal(r({ lokal: null, kandidat: kand(7, 40, "", F(1, 39)) }), "adoptieren", "auch ein Stand ohne Schreiber (alte Seite)");
// 3. Anderes Spiel.
assert.equal(r({ lokal: lokal(6, 10, F(2, 70)), kandidat: kand(7, 40, "B", F(1, 39)) }), "vormerken");
// 4. AK6: eigener Stand kommt zurück — die Revision ordnet, nicht der Fortschritt.
assert.equal(r({ lokal: lokal(7, 41, F(1, 38)), kandidat: kand(7, 40, "A", F(1, 39)) }), "lokal", "Offline-Undo: der zurückgenommene Punkt kommt nicht wieder");
assert.equal(r({ lokal: lokal(7, 40, F(1, 39)), kandidat: kand(7, 40, "A", F(1, 39)) }), "lokal");
assert.equal(r({ lokal: lokal(7, 12, F(0, 12)), kandidat: kand(7, 40, "A", F(1, 39)) }), "adoptieren", "eigener, aber jüngerer Server-Stand");
// 5. AK5: A fällt aus, B zählt weiter, B fällt aus, A kommt zurück → B's Stand gilt.
assert.equal(r({ lokal: lokal(7, 90, F(1, 30)), kandidat: kand(7, 20, "B", F(1, 39)) }), "adoptieren", "fremd und weiter — trotz kleinerer rev");
assert.equal(r({ lokal: lokal(7, 5, F(1, 39)), kandidat: kand(7, 90, "B", F(1, 30)) }), "lokal", "fremd, aber zurück — trotz größerer rev");
assert.equal(r({ lokal: lokal(7, 5, F(1, 39)), kandidat: kand(7, 90, "B", F(1, 39)) }), "lokal", "Gleichstand: lokal");
assert.equal(r({ lokal: lokal(7, 5, F(1, 30)), kandidat: kand(7, 90, "", F(1, 39)) }), "adoptieren", "fehlender Schreiber zählt als fremd");
// Wiedereröffnet schlägt veraltet-beendet: `finished` ist nicht im Fortschritt,
// und beim eigenen Stand ordnet die rev.
assert.equal(r({ lokal: lokal(7, 61, F(2, 66)), kandidat: kand(7, 60, "A", F(2, 67)) }), "lokal");
// Leere eigene Kennung (kein Speicher) gilt nie als „eigener Stand".
assert.equal(w.wahlBeiRestore({ ownershipActive: true, authoritative: true, eigeneKennung: "", lokal: lokal(7, 1, F(0, 1)), kandidat: kand(7, 9, "", F(1, 39)) }), "adoptieren");

// ── Stufe 2: match_assigned ───────────────────────────────────────────
const z = (e) => w.wahlBeiZuweisung({ matchId: 7, lokalMatchId: null, lokalFortschritt: F(0, 0), vorgemerkt: null,
  hostSets: [], hostFortschritt: F(0, 0), ring: null, ...e });
// 1. Vorgemerkter Stand passt.
assert.equal(z({ lokalMatchId: 6, vorgemerkt: { matchId: 7 } }), "adoptieren");
assert.equal(z({ lokalMatchId: 6, vorgemerkt: { matchId: 5 } }), "neu", "vorgemerkt für ein drittes Spiel: verfällt");
// 2. Kein Stand, Host-Sätze leer → 0:0 wie heute.
assert.equal(z({}), "neu");
assert.equal(z({ hostSets: S([0, 0]) }), "neu", "ein 0:0 des Servers ist leer");
// 3. AK2: Ersatzgerät ohne Stand, Server kennt Sätze → Einstieg, nie 0:0.
const HS = S([21, 10], [5, 3]);
assert.equal(z({ hostSets: HS, hostFortschritt: F(1, 39) }), "einstieg");
assert.equal(z({ lokalMatchId: 6, lokalFortschritt: F(2, 80), hostSets: HS, hostFortschritt: F(1, 39) }), "einstieg", "der Stand eines ANDEREN Spiels zählt nicht");
// Selbes Spiel: Server weiter und fremd → Einstieg.
assert.equal(z({ lokalMatchId: 7, lokalFortschritt: F(0, 12), hostSets: HS, hostFortschritt: F(1, 39) }), "einstieg");
// AK6: Server weiter, aber es ist das Echo des eigenen Stands (Undo) → behalten.
assert.equal(z({ lokalMatchId: 7, lokalFortschritt: F(1, 38), hostSets: HS, hostFortschritt: F(1, 39), ring: { matchId: 7, saetze: ["21:10,5:3"] } }), "behalten");
assert.equal(z({ lokalMatchId: 7, lokalFortschritt: F(1, 38), hostSets: HS, hostFortschritt: F(1, 39), ring: { matchId: 6, saetze: ["21:10,5:3"] } }), "einstieg", "Ring eines anderen Spiels gilt nicht");
// 4. Sonst behalten.
assert.equal(z({ lokalMatchId: 7, lokalFortschritt: F(1, 39), hostSets: HS, hostFortschritt: F(1, 39) }), "behalten", "Gleichstand");
assert.equal(z({ lokalMatchId: 7, lokalFortschritt: F(1, 45), hostSets: HS, hostFortschritt: F(1, 39) }), "behalten", "Tablet ist weiter (offline gezählt)");
assert.equal(z({ lokalMatchId: 7, lokalFortschritt: F(1, 45) }), "behalten", "alter Server ohne Host-Sätze (AK9)");

console.log("✓ Stand-Wahl (Modul): alle Prüfungen bestanden");

// ══ Teil 2: der echte Code der Seite ══════════════════════════════════
const hier = dirname(fileURLToPath(import.meta.url));
const seite = readFileSync(join(hier, "..", "src-tauri", "assets", "tablet.html"), "utf8");
function funktion(name) {
  const start = seite.indexOf("function " + name + "(");
  assert.ok(start >= 0, `tablet.html: function ${name} fehlt`);
  let tiefe = 0;
  for (let i = seite.indexOf("{", start); i < seite.length; i++) {
    if (seite[i] === "{") tiefe++;
    else if (seite[i] === "}" && --tiefe === 0) return seite.slice(start, i + 1);
  }
  throw new Error(`tablet.html: function ${name} endet nicht`);
}
const FUNKTIONEN = ["fortschritt", "punkte", "vergleiche", "satzEntschieden", "hostSaetzeLesen", "hostSaetzeTeilen",
  "saetzeSchluessel", "ringMerken", "imRing", "wahlBeiRestore", "wahlBeiZuweisung",
  "ringSichern", "beiStateRestore", "uebernimmStand", "zwischenstandSetzen", "einstiegMitSaetzen",
  "beiMatchAssigned", "chooseSide", "matchWinnerSide", "requiredSetsToWin", "intervalThreshold", "sendScoreUpdate"];

const SPIEL = (id, extra = {}) => ({ matchId: id, targetScore: 21, capScore: 30, bestOfSets: 3, ...extra });

/** Ein Tablet mit der Kennung „A" an Feld 101. */
function tablet(zustand = {}) {
  const ruf = [];
  const gesendet = [];
  const speicher = {};
  const STATE = {
    match: null, rev: 0, setsCompleted: [], currentSet: { left: 0, right: 0 }, teamOnSide: null,
    positions: null, servingSide: null, history: [], rallyLog: null, events: [], finished: false,
    finalized: false, occupied: false, locked: false, pendingResult: null, setup: "pending", pause: null,
    ...zustand,
  };
  const merk = (name) => (...a) => { ruf.push(name); return a; };
  const attrappen = {
    STATE, SCHREIBER: "A", COURT_ID: 101, COURT_LABEL: "Feld 1",
    WebSocket: { OPEN: 1 }, ws: { readyState: 1, send: (x) => gesendet.push(JSON.parse(x)) },
    localStorage: { setItem: (k, v) => { speicher[k] = v; }, getItem: (k) => speicher[k] ?? null },
    tlog: (stufe, was, d) => ruf.push("log:" + was),
    clearReidentify: () => {}, syncServerClock: () => {}, render: merk("render"), clearInterval: () => {},
    persistState: (ausRestore) => ruf.push(ausRestore ? "spiegeln" : "persist"),
    sendRallySync: merk("sendRallySync"), sendMatchEventSync: merk("sendMatchEventSync"),
    trySubmitPending: merk("trySubmitPending"),
    zeigeGeparktMeldung: (text) => ruf.push("hinweis:" + text),
    sendeauftragBeiFinalisierungLoslassen: () => false, geparktBeiFinalisierungAufloesen: () => false,
    // Übernimmt wie die echte Funktion die Felder, auf die es hier ankommt.
    applyPersistedState: (p) => {
      ruf.push("apply");
      Object.assign(STATE, {
        match: p.match, rev: p.rev || 0, setsCompleted: p.setsCompleted || [], currentSet: p.currentSet || { left: 0, right: 0 },
        teamOnSide: p.teamOnSide ?? null, pendingResult: p.pendingResult ?? null, history: p.history || [],
      });
    },
    loadMatch: (m) => {
      ruf.push("loadMatch");
      if (STATE.match && STATE.match.matchId === m.matchId) return;
      Object.assign(STATE, { match: m, setsCompleted: [], currentSet: { left: 0, right: 0 }, teamOnSide: null,
        history: [], rallyLog: null, finished: false, setup: "choose-side", pendingResult: null });
    },
  };
  const namen = Object.keys(attrappen);
  const f = new Function(
    ...namen,
    "const MELDE_RING_MAX = 8; const RING_KEY = 'ring'; let MELDE_RING = null, VORGEMERKT = null;\n" +
      "let healthNachfragenRest = 0, breakTimer = null;\n" +
      FUNKTIONEN.map(funktion).join("\n") +
      "\nreturn { beiStateRestore, beiMatchAssigned, chooseSide, sendScoreUpdate, vorgemerkt: () => VORGEMERKT, ring: () => MELDE_RING };",
  )(...namen.map((n) => attrappen[n]));
  return { STATE, ruf, gesendet, speicher, ...f };
}
const serverStand = (matchId, rev, schreiber, saetze, lauf, extra = {}) => ({
  match: SPIEL(matchId), rev, schreiber,
  setsCompleted: saetze.map(([l, r]) => ({ left: l, right: r })), currentSet: { left: lauf[0], right: lauf[1] },
  teamOnSide: { a: "left", b: "right" }, history: [{ x: 1 }, { x: 2 }], ...extra,
});
const restore = (stand, extra = {}) => ({ type: "state_restore", state: JSON.stringify(stand), ownership_active: true, authoritative: true, ...extra });
const zaehlt = (id, rev, saetze, lauf) => ({
  match: SPIEL(id), rev, setsCompleted: saetze.map(([l, r]) => ({ left: l, right: r })),
  currentSet: { left: lauf[0], right: lauf[1] }, teamOnSide: { a: "left", b: "right" }, setup: "playing",
});
const hat = (t, name) => t.ruf.includes(name);

// ── AK4: frisches Gerät an freiem Feld übernimmt den gesicherten Stand ──
{
  const t = tablet();
  t.beiStateRestore(restore(serverStand(7, 40, "B", [[21, 10]], [5, 3])));
  assert.ok(hat(t, "apply"), "übernommen");
  assert.deepEqual(t.STATE.currentSet, { left: 5, right: 3 });
  assert.equal(t.STATE.history.length, 2, "samt Undo-Schritten");
  assert.ok(t.ruf.indexOf("apply") < t.ruf.indexOf("spiegeln"), "gespiegelt wird erst der ÜBERNOMMENE Stand — nie 0:0");
  assert.equal(t.gesendet.length, 0, "kein score_update mit 0:0");
  // Danach die Zuweisung desselben Spiels: alles bleibt.
  t.beiMatchAssigned({ match: SPIEL(7), hostSets: S([21, 10], [5, 3]) });
  assert.ok(!hat(t, "log:einstieg_mit_host_saetzen"));
  assert.deepEqual(t.STATE.currentSet, { left: 5, right: 3 });
  assert.equal(t.STATE.history.length, 2);
}
{
  // Der übernommene Stand bringt sein eigenes Match-Objekt mit — von einem
  // fremden Gerät. Zählweise und Namen gelten aber vom Turnier-PC (R2).
  const t = tablet();
  const fremd = serverStand(7, 40, "B", [[21, 10]], [5, 3]);
  fremd.match = SPIEL(7, { targetScore: 99, bestOfSets: 1 });
  t.beiStateRestore(restore(fremd));
  assert.equal(t.STATE.match.targetScore, 99, "bis zur Zuweisung steht der fremde Brief");
  t.beiMatchAssigned({ match: SPIEL(7), hostSets: S([21, 10], [5, 3]) });
  assert.equal(t.STATE.match.targetScore, 21, "die Zuweisung setzt den Brief des Turnier-PCs ein");
  assert.equal(t.STATE.match.bestOfSets, 3);
  assert.deepEqual(t.STATE.currentSet, { left: 5, right: 3 }, "der Stand bleibt");
}

// ── AK5: A kommt zurück, B hat weitergezählt → B's Stand gilt ──
{
  const t = tablet(zaehlt(7, 90, [[21, 10]], [4, 2]));
  t.beiStateRestore(restore(serverStand(7, 20, "B", [[21, 10]], [9, 7])));
  assert.ok(hat(t, "apply"));
  assert.deepEqual(t.STATE.currentSet, { left: 9, right: 7 });
  assert.ok(hat(t, "hinweis:Spielstand vom Turnier-PC übernommen — bitte prüfen."), "der Bediener sieht, dass sein Stand ersetzt wurde");
}
{
  // Ohne eigenen Stand (AK4) gibt es nichts zu melden.
  const t = tablet();
  t.beiStateRestore(restore(serverStand(7, 20, "B", [[21, 10]], [9, 7])));
  assert.ok(!t.ruf.some((r) => r.startsWith("hinweis:")));
}
// … und umgekehrt: Der Server-Stand ist fremd, aber zurück → lokal gilt.
{
  const t = tablet(zaehlt(7, 5, [[21, 10]], [9, 7]));
  t.beiStateRestore(restore(serverStand(7, 90, "B", [[21, 10]], [4, 2])));
  assert.ok(!hat(t, "apply"));
  assert.ok(hat(t, "spiegeln"), "der Server-Cache wird mit dem lokalen Stand repariert");
  assert.equal(t.gesendet.at(-1).type, "score_update");
}

// ── AK6: selbes Gerät nach Offline-Undo → der zurückgenommene Punkt bleibt weg ──
{
  const t = tablet(zaehlt(7, 41, [[21, 10]], [5, 2]));
  t.beiStateRestore(restore(serverStand(7, 40, "A", [[21, 10]], [5, 3])));
  assert.ok(!hat(t, "apply"));
  assert.deepEqual(t.STATE.currentSet, { left: 5, right: 2 });
}
// Stufe 2 dazu: Der Server nennt noch 5:3 — es ist das Echo der eigenen Meldung.
{
  const t = tablet(zaehlt(7, 40, [[21, 10]], [5, 3]));
  t.sendScoreUpdate(); // meldet 21:10, 5:3
  assert.deepEqual(t.ring(), { matchId: 7, saetze: ["21:10,5:3"] });
  assert.ok(t.speicher.ring, "der Ring überlebt ein Neuladen");
  t.STATE.currentSet = { left: 5, right: 2 }; // Undo, offline
  t.beiMatchAssigned({ match: SPIEL(7), hostSets: S([21, 10], [5, 3]) });
  assert.ok(!hat(t, "log:einstieg_mit_host_saetzen"), "kein Einstieg mit dem eigenen alten Stand");
  assert.deepEqual(t.STATE.currentSet, { left: 5, right: 2 });
}
// Gegenprobe: Dieselbe Lage OHNE eigene Meldung (fremder Fortschritt) → Einstieg.
{
  const t = tablet(zaehlt(7, 40, [[21, 10]], [5, 2]));
  t.beiMatchAssigned({ match: SPIEL(7), hostSets: S([21, 10], [5, 3]) });
  assert.ok(hat(t, "log:einstieg_mit_host_saetzen"));
}

// ── AK2: Ersatzgerät ohne Stand, kein Server-Cache (Host-Neustart) → Einstieg ──
for (const [seite, sollSaetze, sollLauf] of [
  ["a", [{ left: 21, right: 10 }], { left: 5, right: 3 }],
  ["b", [{ left: 10, right: 21 }], { left: 3, right: 5 }],
]) {
  const t = tablet();
  t.beiMatchAssigned({ match: SPIEL(7), hostSets: S([21, 10], [5, 3]) });
  assert.ok(hat(t, "log:einstieg_mit_host_saetzen"));
  assert.deepEqual(t.STATE.setsCompleted, [{ left: 21, right: 10 }], "vor der Seitenwahl: A = links");
  assert.deepEqual(t.STATE.einstiegAB, { fertig: S([21, 10]), laufend: { a: 5, b: 3 } });
  assert.equal(t.STATE.setup, "choose-side", "die Aufstellung wird neu abgefragt");
  assert.equal(t.STATE.midGameSetup, true);
  assert.deepEqual(t.STATE.history, [], "kein Undo dahinter");
  assert.equal(t.STATE.rallyLog, null, "der Verlauf beginnt hier neu");
  assert.equal(t.gesendet.length, 0, "vor der Seitenwahl geht kein Stand raus — die Host-Sätze bleiben stehen");
  t.chooseSide(seite);
  assert.deepEqual(t.STATE.setsCompleted, sollSaetze, `Seite ${seite}: Sätze stehen auf der richtigen Seite`);
  assert.deepEqual(t.STATE.currentSet, sollLauf);
  assert.equal(t.STATE.setup, "choose-server");
  // Vertippt: zurück in die Seitenwahl (wie der Zurück-Knopf) und anders
  // gewählt — die Sätze müssen der NEUEN Wahl folgen, nicht verdreht bleiben.
  t.STATE.teamOnSide = null; t.STATE.setup = "choose-side";
  t.chooseSide(seite === "a" ? "b" : "a");
  assert.deepEqual(t.STATE.currentSet, seite === "a" ? { left: 3, right: 5 } : { left: 5, right: 3 }, "nach Umentscheiden richtig");
  t.STATE.teamOnSide = null; t.STATE.setup = "choose-side";
  t.chooseSide(seite);
  assert.deepEqual(t.STATE.setsCompleted, sollSaetze, "und wieder zurück: wie beim ersten Mal");
  assert.deepEqual(t.STATE.currentSet, sollLauf);
}
// Die Host-Sätze entscheiden das Spiel schon → nach der Seitenwahl direkt beendet.
{
  const t = tablet();
  t.beiMatchAssigned({ match: SPIEL(7), hostSets: S([21, 10], [21, 12]) });
  t.chooseSide("b");
  assert.equal(t.STATE.einstiegAB, null);
  assert.equal(t.STATE.finished, true);
  assert.equal(t.STATE.setup, "playing");
  assert.deepEqual(t.STATE.setsCompleted, [{ left: 10, right: 21 }, { left: 12, right: 21 }]);
}
// Ohne Host-Sätze (alter Server, AK9) bleibt es bei 0:0 wie bisher.
{
  const t = tablet();
  t.beiMatchAssigned({ match: SPIEL(7) });
  assert.ok(!hat(t, "log:einstieg_mit_host_saetzen"));
  assert.deepEqual(t.STATE.currentSet, { left: 0, right: 0 });
  assert.equal(t.STATE.match.matchId, 7);
}
// Ein finalisiertes Spiel steigt nie ein.
{
  const t = tablet();
  t.beiMatchAssigned({ match: SPIEL(7, { finalized: true }), hostSets: S([21, 10], [5, 3]) });
  assert.ok(!hat(t, "log:einstieg_mit_host_saetzen"));
}

// ── Regel 3: Stand eines anderen Spiels wird vorgemerkt ──
{
  const t = tablet(zaehlt(6, 30, [[21, 10], [21, 12]], [0, 0]));
  const vor = serverStand(7, 40, "B", [[21, 10]], [5, 3]);
  t.beiStateRestore(restore(vor));
  assert.ok(!hat(t, "apply"), "noch nichts übernommen");
  assert.ok(!hat(t, "spiegeln"), "und nichts überschrieben");
  assert.equal(t.vorgemerkt().match.matchId, 7);
  t.beiMatchAssigned({ match: SPIEL(7) });
  assert.ok(hat(t, "apply"), "die Zuweisung bestätigt das Spiel → übernommen");
  assert.ok(t.ruf.indexOf("apply") < t.ruf.indexOf("loadMatch"), "vor dem Laden — sonst begänne es bei 0:0");
  assert.deepEqual(t.STATE.currentSet, { left: 5, right: 3 });
  assert.equal(t.vorgemerkt(), null);
}
{
  // Kommt stattdessen ein drittes Spiel, verfällt der vorgemerkte Stand.
  const t = tablet(zaehlt(6, 30, [[21, 10], [21, 12]], [0, 0]));
  t.beiStateRestore(restore(serverStand(7, 40, "B", [[21, 10]], [5, 3])));
  t.beiMatchAssigned({ match: SPIEL(8) });
  assert.ok(!hat(t, "apply"));
  assert.equal(t.STATE.match.matchId, 8);
  assert.deepEqual(t.STATE.currentSet, { left: 0, right: 0 });
  assert.equal(t.vorgemerkt(), null);
}

// ── AK7: Übernahme (authoritative=false) samt offenem Sendeauftrag ──
{
  const t = tablet(zaehlt(7, 99, [[21, 10]], [20, 3]));
  t.beiStateRestore(restore(serverStand(7, 10, "B", [[21, 10], [21, 12]], [0, 0], { pendingResult: { matchId: 7, sets: [] } }), { authoritative: false }));
  assert.ok(hat(t, "apply"), "Übernahme adoptiert immer");
  assert.ok(hat(t, "trySubmitPending"), "der offene Auftrag wird sofort weiter zugestellt");
}
{
  const t = tablet();
  t.beiStateRestore(restore(serverStand(7, 10, "B", [[21, 10]], [1, 0])));
  assert.ok(!hat(t, "trySubmitPending"), "ohne Auftrag nichts anzustoßen");
}

// ── AK10: Legacy-Schalter — die alte rev-Logik, wortgleich ──
{
  const t = tablet(zaehlt(7, 41, [[21, 10]], [1, 0]));
  t.beiStateRestore(restore(serverStand(7, 40, "B", [[21, 10]], [9, 9]), { ownership_active: false }));
  assert.ok(hat(t, "log:state_restore_kept_local"), "höhere rev gewinnt — auch gegen mehr Fortschritt");
  assert.ok(!hat(t, "apply"));
}
{
  const t = tablet();
  t.beiStateRestore({ type: "state_restore", state: JSON.stringify(serverStand(7, 40, "B", [[21, 10]], [9, 9])), authoritative: true });
  assert.ok(hat(t, "log:state_restore_applied"), "alter Server ohne ownership_active: wie Legacy");
}

// ── Kaputter Zustand: kein Wurf, nichts angefasst ──
{
  const t = tablet(zaehlt(7, 5, [], [3, 2]));
  t.beiStateRestore({ type: "state_restore", state: "kein json", ownership_active: true, authoritative: true });
  t.beiStateRestore({ type: "state_restore", state: "null", ownership_active: true, authoritative: true });
  t.beiStateRestore({ type: "state_restore", state: "7", ownership_active: true, authoritative: true });
  assert.ok(!hat(t, "apply"));
  assert.deepEqual(t.STATE.currentSet, { left: 3, right: 2 });
}

// ── Der Frame-Zweig ruft die geprüften Funktionen; der Spiegel trägt den Schreiber ──
assert.match(seite, /else if \(msg\.type === 'state_restore'\) \{\s*beiStateRestore\(msg\);\s*\}/);
assert.match(funktion("standFuerSpiegel"), /schreiber: SCHREIBER,/);
// Die Schreiber-Kennung ist NICHT die Geräte-Kennung (an der erkennt der
// Server „dasselbe Gerät" und löst die Sitzung ohne Dialog ab).
assert.ok(!/const SCHREIBER = DEVICE_ID/.test(seite), "SCHREIBER ist eine eigene Zufallskennung");
assert.match(seite, /const KEY = 'badhub\.tablet\.schreiber';/);
assert.match(funktion("standFuerSpiegel"), /einstiegAB: STATE\.einstiegAB \|\| null,/);
assert.match(funktion("finalizeSetup"), /STATE\.einstiegAB = null;/, "mit der Aufstellung endet der Einstieg");
assert.match(funktion("loadMatch"), /STATE\.einstiegAB = null;/);
assert.match(funktion("clearMatch"), /STATE\.einstiegAB = null;/);
assert.ok(!/MELDE_RING|VORGEMERKT/.test(funktion("standFuerSpiegel")), "Ring und Vormerkung reisen nicht im Spiegel");

console.log("✓ Stand-Wahl (Seite): alle Prüfungen bestanden");
