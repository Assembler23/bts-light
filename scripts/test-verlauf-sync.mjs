/**
 * Wann schickt das Zähl-Tablet den KOMPLETTEN Abgleich von Punktverlauf
 * und Zettel-Ereignissen? (Spec tablet-stand-robust, Regel D; ADR 0085)
 *
 * Drei Stellen heilen, was ein Verbindungsabriss verschluckt hat:
 *   (a) bei `match_assigned` für das bereits geladene Spiel,
 *   (b) unmittelbar vor dem Senden des Ergebnisses,
 *   (c) einmal nach der Annahme des Ergebnisses.
 *
 * Geprüft wird der ECHTE Code: Die Funktionen werden aus `tablet.html`
 * geschnitten und mit aufzeichnenden Attrappen ausgeführt. Rutscht ein
 * Abgleich hinter das Ergebnis oder hinter den Finalisiert-Rücksprung,
 * schlägt dieser Test an — nicht erst ein Zettel ohne Karte.
 */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const hier = dirname(fileURLToPath(import.meta.url));
const seite = readFileSync(join(hier, "..", "src-tauri", "assets", "tablet.html"), "utf8");

/** Schneidet `function name(…) { … }` aus der Seite (samt `async`). */
function funktion(name) {
  let start = seite.indexOf("function " + name + "(");
  assert.ok(start >= 0, `tablet.html: function ${name} fehlt`);
  const vorsatz = seite.slice(Math.max(0, start - 6), start) === "async " ? "async " : "";
  let tiefe = 0;
  for (let i = seite.indexOf("{", start); i < seite.length; i++) {
    if (seite[i] === "{") tiefe++;
    else if (seite[i] === "}" && --tiefe === 0) return vorsatz + seite.slice(start, i + 1);
  }
  throw new Error(`tablet.html: function ${name} endet nicht`);
}

/** Baut die echten Funktionen in eine Umgebung aus Attrappen. Jede Attrappe
 *  schreibt ihren Namen ins Protokoll `ruf`. */
function umgebung({ state, fetchAntwort, echtesLoadMatch = false }) {
  const ruf = [];
  const merk = (name) => (...a) => { ruf.push(name); return a; };
  const attrappen = {
    STATE: state,
    tlog: (stufe, was) => ruf.push("log:" + was),
    clearReidentify: merk("clearReidentify"),
    syncServerClock: merk("syncServerClock"),
    sendeauftragBeiFinalisierungLoslassen: () => { ruf.push("loslassen"); state.pendingResult = null; return true; },
    render: merk("render"),
    sendScoreUpdate: merk("sendScoreUpdate"),
    persistState: merk("persistState"),
    sendRallySync: merk("sendRallySync"),
    sendMatchEventSync: merk("sendMatchEventSync"),
    uploadTabletLog: merk("uploadTabletLog"),
    // Parken (Regel C) ist hier nicht Thema — eigene Prüfung in test-geparkt.mjs.
    GEPARKT: [],
    parkeFremdenAuftrag: () => false,
    geparktFuer: () => null,
    geparktAntwort: () => false,
    planeGeparktPruefung: () => {},
    geparktBeiFinalisierungAufloesen: merk("geparktAufloesen"),
    einstiegMitSaetzen: merk("einstieg"),
    uebernimmStand: merk("uebernimm"),
    naechsterSchritt: (data) => (data && data.ok ? { art: "ok" } : { art: "wiederholen", grund: "nein" }),
    fetch: async () => { ruf.push("fetch"); return fetchAntwort(); },
    COURT_ID: 101, COURT_LABEL: "Feld 1", BASE_PATH: "", RESULT_ABORT_MS: 1000, RESULT_RETRY_MS: 60_000,
    // Wiederholungs-Timer nicht wirklich stellen — der Test soll enden.
    setTimeout: (f, ms) => { if (ms === 60_000) { ruf.push("retry_geplant"); return 0; } return setTimeout(f, ms); },
    clearTimeout,
    AbortController,
  };
  if (!echtesLoadMatch) {
    attrappen.loadMatch = (m) => {
      ruf.push("loadMatch");
      if (state.match && state.match.matchId === m.matchId) return;
      // Neues Spiel: frischer Stand, wie die echte Funktion.
      state.match = m; state.rallyLog = null; state.events = []; state.finished = false; state.pendingResult = null;
    };
  }
  const namen = Object.keys(attrappen);
  const bau = new Function(
    ...namen,
    "let healthNachfragenRest = 0;\nlet resultRetryTimer = null;\n" +
      "const MELDE_RING_MAX = 8; let MELDE_RING = null, VORGEMERKT = null;\n" +
      ["beiMatchAssigned", "submit", "trySubmitPending",
        // Stand-Wahl (Regel A) — hier ohne Host-Sätze, also ohne Wirkung.
        "fortschritt", "punkte", "vergleiche", "satzEntschieden", "hostSaetzeLesen", "hostSaetzeTeilen", "saetzeSchluessel", "ringMerken", "imRing", "wahlBeiRestore", "wahlBeiZuweisung"].map(funktion).join("\n") +
      "\nreturn { beiMatchAssigned, submit, trySubmitPending };",
  );
  return { ruf, ...bau(...namen.map((n) => attrappen[n])) };
}

function zustand(extra = {}) {
  return {
    match: { matchId: 7 }, finalized: false, finished: true, occupied: false,
    teamOnSide: { a: "left", b: "right" }, setsCompleted: [{ left: 21, right: 10 }, { left: 21, right: 12 }],
    pendingResult: null, submittedOk: false, resultRejected: null,
    retired: false, walkover: false, cascadeWalkover: false, retiredWinner: null,
    rallyLog: { sets: [], midGame: false }, events: [],
    ...extra,
  };
}

const pos = (ruf, name) => {
  const i = ruf.indexOf(name);
  assert.ok(i >= 0, `${name} wurde nicht gerufen (Protokoll: ${ruf.join(", ")})`);
  return i;
};
const anzahl = (ruf, name) => ruf.filter((r) => r === name).length;

// ── (a) match_assigned fürs bereits geladene Spiel ────────────────────
{
  const state = zustand({ finished: false });
  const { ruf, beiMatchAssigned } = umgebung({ state });
  beiMatchAssigned({ type: "match_assigned", match: { matchId: 7 } });
  assert.equal(anzahl(ruf, "sendRallySync"), 1, "Verlauf einmal komplett");
  assert.equal(anzahl(ruf, "sendMatchEventSync"), 1, "Ereignisse einmal komplett");
  assert.ok(pos(ruf, "sendRallySync") > pos(ruf, "loadMatch"), "erst laden, dann abgleichen");
  assert.ok(pos(ruf, "sendRallySync") < pos(ruf, "sendMatchEventSync"), "Verlauf vor Ereignissen");
  assert.ok(pos(ruf, "sendScoreUpdate") > pos(ruf, "sendMatchEventSync"), "Stand-Meldung danach wie bisher");
  assert.equal(state.finalized, false);
}

// (a) auch wenn das Spiel inzwischen von Hand in BTP beendet wurde: Der
// Abgleich geht VOR dem Rücktritt raus, danach meldet das Tablet nichts mehr.
{
  const state = zustand({ finished: false, pendingResult: { matchId: 7 } });
  const { ruf, beiMatchAssigned } = umgebung({ state });
  beiMatchAssigned({ type: "match_assigned", match: { matchId: 7, finalized: true } });
  assert.equal(state.finalized, true);
  assert.ok(pos(ruf, "sendRallySync") < pos(ruf, "log:match_finalized_stand_down"), "Abgleich vor dem Rücktritt");
  assert.ok(pos(ruf, "sendMatchEventSync") < pos(ruf, "loslassen"));
  assert.equal(anzahl(ruf, "sendScoreUpdate"), 0, "finalisiert: kein Stand mehr");
  assert.equal(anzahl(ruf, "persistState"), 0, "finalisiert: kein Spiegel mehr");
}

// Ein ANDERES Spiel kommt: ein letzter Abgleich des alten — unter der ALTEN
// Kennung, also VOR `loadMatch` — und danach keiner mehr (der alte Verlauf
// darf nie unter der neuen Match-ID rausgehen).
{
  const state = zustand({ finished: false });
  const { ruf, beiMatchAssigned } = umgebung({ state });
  beiMatchAssigned({ type: "match_assigned", match: { matchId: 8 } });
  assert.equal(anzahl(ruf, "sendRallySync"), 1, "ein letzter Verlaufs-Abgleich");
  assert.equal(anzahl(ruf, "sendMatchEventSync"), 1, "ein letzter Ereignis-Abgleich");
  assert.ok(pos(ruf, "sendRallySync") < pos(ruf, "loadMatch"), "vor dem Laden des neuen Spiels");
  assert.ok(pos(ruf, "sendMatchEventSync") < pos(ruf, "loadMatch"));
  assert.equal(state.match.matchId, 8);
  assert.equal(pos(ruf, "sendScoreUpdate") >= 0, true);
}

// (a) negativ: Tablet ohne Spiel bekommt eines zugewiesen.
{
  const state = zustand({ match: null, finished: false });
  const { ruf, beiMatchAssigned } = umgebung({ state });
  beiMatchAssigned({ type: "match_assigned", match: { matchId: 7 } });
  assert.equal(anzahl(ruf, "sendRallySync"), 0);
}

// ── (b) + (c): rund ums Ergebnis ──────────────────────────────────────
{
  const state = zustand();
  const { ruf, submit } = umgebung({
    state,
    fetchAntwort: () => ({ ok: true, json: async () => ({ ok: true }) }),
  });
  submit();
  // Synchroner Teil: Abgleich steht VOR dem fetch.
  assert.ok(pos(ruf, "sendRallySync") < pos(ruf, "fetch"), "(b) Verlauf vor dem Ergebnis");
  assert.ok(pos(ruf, "sendMatchEventSync") < pos(ruf, "fetch"), "(b) Ereignisse vor dem Ergebnis");
  assert.ok(pos(ruf, "log:submit_start") < pos(ruf, "sendRallySync"));
  assert.deepEqual(state.pendingResult.sets, [{ a: 21, b: 10 }, { a: 21, b: 12 }]);
  await new Promise((r) => setTimeout(r, 20));
  assert.equal(state.submittedOk, true);
  assert.equal(state.pendingResult, null);
  const ok = pos(ruf, "log:submit_ok");
  const danach = ruf.slice(ok);
  assert.equal(anzahl(danach, "sendRallySync"), 1, "(c) genau ein Verlaufs-Abgleich nach der Annahme");
  assert.equal(anzahl(danach, "sendMatchEventSync"), 1, "(c) genau ein Ereignis-Abgleich nach der Annahme");
  assert.equal(anzahl(ruf, "sendRallySync"), 2, "insgesamt (b) + (c)");
}

// (c) negativ: Absage → kein Abgleich hinterher, aber ein neuer Versuch.
{
  const state = zustand();
  const { ruf, submit } = umgebung({
    state,
    fetchAntwort: () => ({ ok: true, json: async () => ({ ok: false }) }),
  });
  submit();
  await new Promise((r) => setTimeout(r, 20));
  assert.equal(state.submittedOk, false);
  assert.ok(state.pendingResult, "Auftrag bleibt offen");
  assert.equal(anzahl(ruf, "sendRallySync"), 1, "nur (b)");
  assert.equal(anzahl(ruf, "retry_geplant"), 1);
}

// (c) negativ: Ein Wiederholungslauf (Timer) gleicht vorher NICHT erneut ab —
// sonst ginge alle fünf Sekunden der volle Verlauf raus.
{
  const state = zustand({ pendingResult: { matchId: 7, sets: [] } });
  const { ruf, trySubmitPending } = umgebung({
    state,
    fetchAntwort: () => ({ ok: false, status: 502 }),
  });
  await trySubmitPending();
  assert.equal(anzahl(ruf, "sendRallySync"), 0);
  assert.equal(anzahl(ruf, "retry_geplant"), 1);
}

// (c) negativ: Während des Wartens kam ein anderes Spiel aufs Feld — die
// späte Annahme gleicht nichts ab (es wäre der Verlauf des falschen Spiels).
{
  const state = zustand();
  let freigabe;
  const { ruf, submit } = umgebung({
    state,
    fetchAntwort: () => new Promise((r) => { freigabe = () => r({ ok: true, json: async () => ({ ok: true }) }); }),
  });
  submit();
  await new Promise((r) => setTimeout(r, 5));
  const offen = state.pendingResult;
  state.match = { matchId: 8 };
  freigabe();
  await new Promise((r) => setTimeout(r, 20));
  assert.equal(offen.matchId, 7);
  assert.equal(state.submittedOk, true, "die Annahme selbst gilt weiter dem Auftrag");
  assert.equal(anzahl(ruf, "sendRallySync"), 1, "nur (b), kein (c) fürs fremde Spiel");
}

// Finalisiert: `submit` tut nichts — auch keinen Abgleich.
{
  const state = zustand({ finalized: true });
  const { ruf, submit } = umgebung({ state, fetchAntwort: () => { throw new Error("kein fetch"); } });
  submit();
  assert.equal(anzahl(ruf, "sendRallySync"), 0);
  assert.equal(anzahl(ruf, "fetch"), 0);
}

// ── Der Abgleich trägt `finished`/`retired` — daran hängt der Host ────
{
  const text = funktion("sendRallySync");
  assert.match(text, /finished: !!STATE\.finished/, "rally_sync trägt finished");
  assert.match(text, /retired: !!STATE\.retired/, "rally_sync trägt retired");
  assert.match(text, /type: 'rally_sync'/);
}

// ── Der Frame-Zweig ruft die geprüfte Funktion ────────────────────────
assert.match(
  seite,
  /if \(msg\.type === 'match_assigned'\) \{\s*beiMatchAssigned\(msg\);\s*\}/,
  "ws.onmessage reicht match_assigned an beiMatchAssigned",
);

console.log("✓ Verlaufs-Abgleich: alle Prüfungen bestanden");
