/**
 * Größen-Regression des gespiegelten Tablet-Zustands (Spec
 * tablet-stand-robust, AK11; ADR 0084).
 *
 * Der Zustand reist je Punkt an den Turnier-PC bzw. das Relay; über
 * 64 KiB wird er dort verworfen. Vor Format 2 lag er am Ende eines
 * normalen Dreisätzers bei 61–66 KB und im Schiri-Modus bei bis zu 590 KB.
 *
 * Gemessen wird der ECHTE Code: `snapshot()` und `standFuerSpiegel()` werden
 * aus `tablet.html` geschnitten und auf einem simulierten Spiel ausgeführt —
 * kommt dort ein Feld dazu, das je Undo-Stand kopiert wird, schlägt dieser
 * Test an, nicht erst das Relay in der Halle.
 */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { SPIEGEL_BUDGET } from "../src/io/undoStand.mjs";

const hier = dirname(fileURLToPath(import.meta.url));
const seite = readFileSync(join(hier, "..", "src-tauri", "assets", "tablet.html"), "utf8");
const proto = readFileSync(join(hier, "..", "relay-proto", "src", "lib.rs"), "utf8");

function funktion(text, name) {
  const start = text.indexOf("function " + name + "(");
  assert.ok(start >= 0, `tablet.html: function ${name} fehlt`);
  let tiefe = 0;
  for (let i = text.indexOf("{", start); i < text.length; i++) {
    if (text[i] === "{") tiefe++;
    else if (text[i] === "}" && --tiefe === 0) return text.slice(start, i + 1);
  }
  throw new Error(`tablet.html: function ${name} endet nicht`);
}

// ── Deckel: Seite, Modul und relay-proto passen zusammen ──────────────
const budgetSeite = Number(seite.match(/const SPIEGEL_BUDGET = (\d+);/)?.[1]);
assert.equal(budgetSeite, SPIEGEL_BUDGET, "SPIEGEL_BUDGET: Seite wie Modul");
const deckel = proto.match(/pub const MAX_STATE_LEN: usize = (\d+) \* (\d+);/);
assert.ok(deckel, "relay-proto: MAX_STATE_LEN = a * b");
const MAX_STATE_LEN = Number(deckel[1]) * Number(deckel[2]);
// Beide zählen UTF-8-Bytes; die Reserve gilt dem Frame-Rahmen.
assert.ok(SPIEGEL_BUDGET <= MAX_STATE_LEN - 8000, "Budget (Bytes) lässt Reserve unter dem Deckel");

// ── Die echten Funktionen der Seite ───────────────────────────────────
const bau = new Function(
  "STATE",
  "function computeServing() { return { team: 'a', index: 0 }; }\n" +
    "function computeReceiving() { return { team: 'b', index: 1 }; }\n" +
    ["verlaufGueltig", "verlaufSchnitt", "snapshot", "standFuerSpiegel"].map((n) => funktion(seite, n)).join("\n") +
    "\nreturn { snapshot, standFuerSpiegel };",
);

/** Ein Doppel mit langen Namen, Vereinen und Umlauten — so, wie der
 *  `MatchBrief` es dem Tablet liefert (obere Grenze des Üblichen). */
function neuerZustand() {
  const sp = (i) => ({
    id: 1000 + i,
    name: "Maximilian-Jürgen Müller-Lüdenscheidt " + i,
    nationality: "GER",
    club: "SG Grün-Weiß Großbeeren/Blankenfelde 1921 e. V.",
    licence: "01-1234" + i,
  });
  return {
    rev: 0,
    match: {
      matchId: 4711, courtId: 101, court: "Feld 12", matchName: "HD-A · Viertelfinale", matchNr: 123,
      eventName: "Herrendoppel A", roundName: "Viertelfinale", discipline: "HD",
      teamA: [sp(1), sp(2)], teamB: [sp(3), sp(4)],
      bestOf: 3, targetScore: 21, capScore: 30, finalized: false, teamsCorrection: true, feldCheck: true,
      srNames: ["Schiedsrichterin Müller-Öztürk"], arNames: ["Aufschlagrichter Straßenburg"],
      scorekeeper: ["Zähltafelbediener Eins", "Zähltafelbediener Zwei"], seedA: "1", seedB: "3/4",
      hall: "Halle 2 (Sporthalle Süd)", hallColor: "#2b7fff",
    },
    setsCompleted: [], currentSet: { left: 0, right: 0 },
    teamOnSide: { a: "left", b: "right" },
    positions: { left: { rightSC: 1001, leftSC: 1002 }, right: { rightSC: 1003, leftSC: 1004 } },
    servingSide: "left", setup: "playing", serveSetupTeam: null, midGameSetup: false, berichtigtAbfrage: false,
    finished: false, midGameSwitchDone: false, pause: null, intervalDoneThisGame: false,
    submittedOk: false, pendingResult: null, resultRejected: null, startedAt: 1_760_000_000_000,
    alertInjury: false, alertOfficial: false, retired: false, walkover: false, cascadeWalkover: false,
    retiredWinner: null, history: [], rallyLog: null, events: [], eventSeq: 0,
  };
}

function ereignis(S, kind) {
  S.eventSeq++;
  S.events.push({
    id: "a1b2c3d4e5f6" + S.eventSeq, seq: S.eventSeq, set: S.setsCompleted.length + 1, afterN: 7,
    scoreA: 11, scoreB: 9, tsMs: 1_760_000_123_456, kind, team: 1, player: 1002,
    receiverTeam: 2, receiverPlayer: 1003, phase: "play",
  });
}

/** Spielt die Sätze durch und gibt die größte Zustandslänge in Bytes. */
function groesste(saetze, zusatzJeSatz) {
  const S = neuerZustand();
  const f = bau(S);
  let max = 0;
  for (let si = 0; si < saetze.length; si++) {
    const [zielA, zielB] = saetze[si];
    ereignis(S, "serve_start");
    for (let k = 0; k < zusatzJeSatz; k++) ereignis(S, k % 2 ? "card_yellow" : "card_red");
    if (!S.rallyLog) S.rallyLog = { sets: [], midGame: false };
    S.rallyLog.sets.push({ startA: 0, startB: 0, points: "" });
    let a = 0, b = 0;
    for (let n = 0; n < zielA + zielB; n++) {
      S.history.push(f.snapshot());
      if (S.history.length > 50) S.history.shift();
      const w = a < zielA && (b >= zielB || n % 2 === 0) ? "A" : "B";
      if (w === "A") a++; else b++;
      S.rallyLog.sets[si].points += w;
      S.currentSet = { left: a, right: b };
      S.rev++;
      max = Math.max(max, Buffer.byteLength(JSON.stringify(f.standFuerSpiegel())));
    }
    S.setsCompleted.push({ left: a, right: b });
    S.currentSet = { left: 0, right: 0 };
  }
  return { max, S, f };
}

const KB = 1000;
const FAELLE = [
  ["Doppel 3 × 21:19", [[21, 19], [19, 21], [21, 19]], 0, 28 * KB],
  ["Doppel 3 × 30:29", [[30, 29], [29, 30], [30, 29]], 0, 28 * KB],
  // Deckel der Zettel-Ereignisse ist 64: 3 Aufschlagfolgen + 60 Karten.
  ["Doppel 3 × 30:29, 63 Ereignisse", [[30, 29], [29, 30], [30, 29]], 20, 40 * KB],
];
for (const [name, saetze, zusatz, grenze] of FAELLE) {
  const { max, S } = groesste(saetze, zusatz);
  console.log(`${name}: größter Zustand ${(max / KB).toFixed(1)} KB (Grenze ${grenze / KB} KB)`);
  assert.ok(max <= grenze, `${name}: ${max} B > ${grenze} B`);
  assert.ok(max <= SPIEGEL_BUDGET, `${name}: nie gekürzt`);
  assert.equal(S.history.length, 50, `${name}: volle 50 Undo-Stände`);
}

// ── Kein Undo-Stand trägt Kopien, die mit dem Spiel wachsen ───────────
{
  const { S, f } = groesste([[30, 29], [29, 30], [30, 29]], 20);
  const stand = f.snapshot();
  assert.ok(!("events" in stand), "Undo-Stand ohne Ereignis-Kopie");
  assert.ok(!("rallyLog" in stand), "Undo-Stand ohne Verlaufs-Vollkopie");
  assert.deepEqual(stand.rl, [[59, 59, 59], false]);
  const je = Buffer.byteLength(JSON.stringify(stand));
  assert.ok(je < 500, `ein Undo-Stand bleibt klein (${je} B)`);
  assert.equal(f.standFuerSpiegel().fmt, 2, "Format-Marke");
  assert.equal(S.events.length, 63);
}

// ── AK12: das echte persistState() kürzt NUR den Spiegel und meldet einmal ──
{
  const S = neuerZustand();
  const gesendet = [], log = [], speicher = {};
  const WebSocket = { OPEN: 1 };
  const persistState = new Function(
    "STATE", "ws", "WebSocket", "localStorage", "tlog", "PERSIST_KEY", "performance",
    "function computeServing() { return null; }\nfunction computeReceiving() { return null; }\n" +
      "const SPIEGEL_BUDGET = " + SPIEGEL_BUDGET + ";\n" +
      "const standWarnung = { speicher: undefined, gekuerzt: undefined, langsam: undefined };\n" +
      ["utf8Laenge", "verlaufGueltig", "verlaufSchnitt", "spiegelKuerzen", "standFuerSpiegel", "persistState"]
        .map((n) => funktion(seite, n)).join("\n") +
      "\nreturn persistState;",
  )(
    S, { readyState: 1, send: (t) => gesendet.push(JSON.parse(t)) }, WebSocket,
    { setItem: (k, v) => { speicher[k] = v; } }, (stufe, was, d) => log.push([stufe, was, d]), "K", { now: () => 0 },
  );
  // Künstlich zu groß: 50 Undo-Stände mit je 2 KB Ballast (kann das neue
  // Format selbst nicht erzeugen — genau dafür ist die Kürzung da).
  for (let i = 0; i < 50; i++) S.history.push({ nr: i, ballast: "ä".repeat(1000) });
  persistState();
  persistState();
  S.currentSet = { left: 1, right: 0 };
  persistState();
  assert.equal(gesendet.length, 3);
  for (const frame of gesendet) {
    assert.equal(frame.type, "state_sync");
    assert.ok(Buffer.byteLength(frame.state) <= SPIEGEL_BUDGET, "Spiegel passt ins Budget (Bytes)");
    assert.ok(Buffer.byteLength(frame.state) <= MAX_STATE_LEN, "und unter den Deckel");
    const h = JSON.parse(frame.state).history;
    assert.ok(h.length > 0 && h.length < 50 && h[h.length - 1].nr === 49, "die jüngsten Undo-Stände bleiben");
  }
  assert.equal(JSON.parse(speicher.K).history.length, 50, "lokal bleibt alles");
  assert.ok(Buffer.byteLength(speicher.K) > MAX_STATE_LEN, "der lokale Stand IST über dem Deckel");
  const warn = log.filter(([, was]) => was === "spiegel_gekuerzt");
  assert.equal(warn.length, 1, "genau eine Warnung je Spiel");
  assert.equal(warn[0][0], "warn");
  assert.deepEqual(Object.keys(warn[0][2]).sort(), ["gespiegelt", "len", "matchId"], "nur Kennzahlen, kein Inhalt");
  // Nächstes Spiel → wieder eine Warnung.
  S.match = { ...S.match, matchId: 4712 };
  persistState();
  assert.equal(log.filter(([, was]) => was === "spiegel_gekuerzt").length, 2);
}

console.log("✓ Stand-Größe: alle Prüfungen bestanden");
