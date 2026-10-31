/**
 * Seiten und Mannschaften am Zähl-Tablet korrigieren (Spec
 * tablet-seiten-korrigieren, ADR 0065).
 *
 * Prüft die kanonische Fassung aus `src/io/seitenKorrektur.mjs`. Die
 * Inline-Kopie in `src-tauri/assets/tablet.html` muss gemeinsam mit ihr
 * geändert werden.
 */
import assert from "node:assert/strict";
import {
  MAX_EVENTS,
  seitenGetauscht,
  mannschaftenGetauscht,
  standAB,
  korrekturErlaubt,
  korrekturAnker,
  platzFuerKorrektur,
  brauchtAbfrage,
} from "../src/io/seitenKorrektur.mjs";

const einzel = {
  matchId: 7,
  teamA: [{ id: 1, name: "Anna" }],
  teamB: [{ id: 11, name: "Ben" }],
};
const doppel = {
  matchId: 8,
  teamA: [
    { id: 1, name: "Anna" },
    { id: 2, name: "Alma" },
  ],
  teamB: [
    { id: 11, name: "Ben" },
    { id: 12, name: "Bert" },
  ],
};

/** Einzel, Satz 1 21:15 für A, laufend 5:3 — A steht links, A schlägt auf. */
function laufend() {
  return {
    match: einzel,
    teamOnSide: { a: "left", b: "right" },
    positions: { left: { rightSC: 1, leftSC: 1 }, right: { rightSC: 11, leftSC: 11 } },
    currentSet: { left: 5, right: 3 },
    setsCompleted: [{ left: 21, right: 15 }],
    servingSide: "left",
    serveSetupTeam: null,
    rallyLog: {
      sets: [
        { startA: 0, startB: 0, points: "AAB" },
        { startA: 0, startB: 0, points: "ABAAB" },
      ],
      midGame: false,
    },
    cards: [{ team: "a", index: 0, name: "Anna", color: "yellow" }],
    retiredWinner: null,
    finished: false,
  };
}
const mit = (z, teil) => ({ ...z, ...teil });

// ── Seiten tauschen ───────────────────────────────────────────────────
{
  const z = laufend();
  const t = seitenGetauscht(z);
  assert.deepEqual(t.teamOnSide, { a: "right", b: "left" });
  assert.deepEqual(t.currentSet, { left: 3, right: 5 }, "Punkte wandern mit");
  assert.deepEqual(t.setsCompleted, [{ left: 15, right: 21 }]);
  assert.equal(t.servingSide, "right", "Aufschläger bleibt derselbe Spieler");
  assert.equal(t.positions.right.rightSC, 1);
  // Der gemeldete Stand ändert sich NICHT.
  assert.deepEqual(standAB(mit(z, t)), standAB(z));
  // Zweimal getauscht = unverändert.
  const zurueck = seitenGetauscht(mit(z, t));
  for (const k of Object.keys(zurueck)) assert.deepEqual(zurueck[k], z[k], k);
  // Eingabe bleibt unberührt.
  assert.deepEqual(z, laufend());
  assert.equal(seitenGetauscht(mit(z, { teamOnSide: null })), null);
}

// ── Mannschaften tauschen (Einzel) ────────────────────────────────────
{
  const z = laufend();
  const t = mannschaftenGetauscht(z);
  assert.deepEqual(t.teamOnSide, { a: "right", b: "left" });
  // Punkte bleiben auf der Seite → gemeldet kippt A:B auf B:A.
  const vorher = standAB(z);
  const nachher = standAB(mit(z, t));
  assert.deepEqual(vorher.aktuell, { a: 5, b: 3 });
  assert.deepEqual(nachher.aktuell, { a: 3, b: 5 });
  assert.deepEqual(nachher.saetze, [{ a: 15, b: 21 }], "auch fertige Sätze");
  // Einzel: Spieler-IDs eindeutig umgemappt — links steht jetzt Ben.
  assert.deepEqual(t.positions.left, { rightSC: 11, leftSC: 11 });
  assert.deepEqual(t.positions.right, { rightSC: 1, leftSC: 1 });
  // Punktverlauf in A/B kippt.
  assert.equal(t.rallyLog.sets[1].points, "BABBA");
  assert.equal(t.rallyLog.midGame, false);
  // Karte bleibt auf ihrer Seite, die Person wechselt mit.
  assert.deepEqual(t.cards[0], { team: "b", index: 0, name: "Ben", color: "yellow" });
  // Aufgabe/Kampflos: Sieger neu wählen.
  assert.equal(mannschaftenGetauscht(mit(z, { retiredWinner: "a" })).retiredWinner, null);
  // Zweimal getauscht = unverändert (bis auf den Sieger).
  const zz = mit(z, t);
  const zurueck = mannschaftenGetauscht(zz);
  for (const k of ["teamOnSide", "positions", "rallyLog", "cards"]) {
    assert.deepEqual(zurueck[k], z[k], k);
  }
  assert.deepEqual(z, laufend(), "Eingabe unberührt");
}

// ── Mannschaften tauschen (Doppel) ────────────────────────────────────
{
  const z = mit(laufend(), {
    match: doppel,
    positions: { left: { rightSC: 2, leftSC: 1 }, right: { rightSC: 12, leftSC: 11 } },
    serveSetupTeam: "a",
  });
  const t = mannschaftenGetauscht(z);
  // Platzhalter nach Listenposition, bis die Neuabfrage greift.
  assert.deepEqual(t.positions.left, { rightSC: 12, leftSC: 11 });
  assert.equal(t.serveSetupTeam, "b", "anstehende Satzbeginn-Abfrage kippt mit");
  assert.equal(brauchtAbfrage(laufend(), true), true);
  assert.equal(brauchtAbfrage(laufend(), false), false, "Einzel: nie");
  assert.equal(brauchtAbfrage(z, true), false, "Satzbeginn-Abfrage steht schon an");
  assert.equal(brauchtAbfrage(mit(laufend(), { finished: true }), true), false);
}

// ── Freigabe ──────────────────────────────────────────────────────────
{
  const ok = {
    match: einzel,
    teamOnSide: { a: "left", b: "right" },
    setup: "playing",
  };
  assert.equal(korrekturErlaubt(ok), true);
  assert.equal(korrekturErlaubt({ ...ok, finished: true }), true, "Ergebnis-Ansicht");
  assert.equal(korrekturErlaubt({ ...ok, resultRejected: "x" }), true);
  for (const sperre of ["pendingResult", "submittedOk", "finalized", "locked", "occupied"]) {
    assert.equal(korrekturErlaubt({ ...ok, [sperre]: true }), false, sperre);
  }
  for (const setup of ["choose-side", "choose-server", "choose-receiver", "pending"]) {
    assert.equal(korrekturErlaubt({ ...ok, setup }), false, setup);
  }
  assert.equal(korrekturErlaubt({ ...ok, teamOnSide: null }), false);
  assert.equal(korrekturErlaubt({ ...ok, berichtigtAbfrage: true }), false, "Neuabfrage offen");
  assert.equal(korrekturErlaubt({ ...ok, match: null }), false);
}

// ── Anker ─────────────────────────────────────────────────────────────
{
  const z = laufend();
  assert.deepEqual(korrekturAnker(z), { set: 2, afterN: 5 });
  // Beendet nach fünf Sätzen: Satz 5, nie 6.
  const fuenf = mit(z, {
    finished: true,
    setsCompleted: [1, 2, 3, 4, 5].map(() => ({ left: 21, right: 19 })),
    rallyLog: {
      sets: [1, 2, 3, 4, 5].map(() => ({ startA: 0, startB: 0, points: "AB" })),
    },
  });
  assert.deepEqual(korrekturAnker(fuenf), { set: 5, afterN: 2 });
  // Karte aus der Satzpause steht schon an (n+1, 0) — der Anker nie davor.
  const karte = mit(fuenf, { events: [{ id: "k", set: 6, afterN: 0 }] });
  assert.deepEqual(korrekturAnker(karte), { set: 6, afterN: 0 });
  // Zurückgenommenes verschiebt den Anker nicht.
  const zurueck = mit(fuenf, {
    events: [
      { id: "k", set: 6, afterN: 0 },
      { id: "r", kind: "retract", retracts: "k", set: 6, afterN: 0 },
    ],
  });
  assert.deepEqual(korrekturAnker(zurueck), { set: 5, afterN: 2 });
  // Laufendes Spiel: nur der Stand zählt (ein zurückgenommenes Ereignis
  // hinter dem Stand darf das nächste Undo nicht auf die Berichtigung lenken).
  assert.deepEqual(korrekturAnker(mit(z, { events: [{ id: "k", set: 2, afterN: 9 }] })), {
    set: 2,
    afterN: 5,
  });
  // Ohne Verlauf: afterN 0.
  assert.deepEqual(korrekturAnker(mit(z, { rallyLog: null })), { set: 2, afterN: 0 });
}

// ── Deckel ────────────────────────────────────────────────────────────
{
  const voll = (n) => Array.from({ length: n }, (_, i) => ({ id: String(i) }));
  assert.equal(platzFuerKorrektur(voll(MAX_EVENTS - 2), false), true);
  assert.equal(platzFuerKorrektur(voll(MAX_EVENTS - 1), false), false);
  assert.equal(platzFuerKorrektur(voll(MAX_EVENTS - 4), true), true);
  assert.equal(platzFuerKorrektur(voll(MAX_EVENTS - 3), true), false);
  assert.equal(platzFuerKorrektur(undefined, true), true);
}

console.log("test-seiten-korrektur: ok");
