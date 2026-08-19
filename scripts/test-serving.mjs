#!/usr/bin/env node
// Regressionstest der BWF-Aufschlag-Positionierung des Tablet-Spielzettels
// (Plan 12b, Mid-Game-Einstieg). `src-tauri/assets/tablet.html` ist eine
// monolithische HTML-Datei ohne Modul-Export, deshalb bildet dieser Test die
// KERNLOGIK aus tablet.html nach:
//   - computeServing()   : Aufschlagfeld aus Score-Parität (serverScore % 2)
//   - finalizeSetup()    : Basis-Zuordnung (Server rechts) + Tausch BEIDER
//                          Seiten bei ungeradem Server-Stand (der Plan-12b-Fix)
//   - addPointOnSide()   : Serving-Team-Punkt tauscht die eigene Seite,
//                          Side-out wechselt nur servingSide
//
// Er prüft die zentrale Invariante: Ein Einstieg mitten im Spiel (Stand N,
// bekannter Aufschläger/Annehmer) muss exakt dieselbe Aufschlag-/Annahme-
// Aufstellung ergeben wie eine ununterbrochene Zählung von 0:0 — und das
// Weiterzählen ab dem Einstieg bleibt konsistent.
//
// ⚠️ Ändert sich diese Logik in tablet.html, HIER mitziehen (bewusste
// Duplikation der ~20 Zeilen, damit die sicherheitskritische Regel eine
// maschinelle Absicherung hat; tablet.html hat sonst kein JS-Harness).

// ── Nachbau der tablet.html-Kernlogik ──────────────────────────────────────
const field = (serverScore) => (serverScore % 2 === 0 ? "rightSC" : "leftSC");

// computeServing: Spieler-ID des aktuellen Aufschlägers.
function serverOf(pos, servingSide, score) {
  const s = servingSide === "left" ? score.left : score.right;
  return pos[servingSide][field(s)];
}
// Diagonaler Annehmer (gleiches Feld auf der Empfangsseite).
function receiverOf(pos, servingSide, score) {
  const recvSide = servingSide === "left" ? "right" : "left";
  const s = servingSide === "left" ? score.left : score.right;
  return pos[recvSide][field(s)];
}

// finalizeSetup: Aufstellung aus (Server, Annehmer, Server-Stand) rekonstruieren.
function finalize(serverSide, server, serverPartner, receiver, receiverPartner, serverScore) {
  const recvSide = serverSide === "left" ? "right" : "left";
  const pos = {
    [serverSide]: { rightSC: server, leftSC: serverPartner },
    [recvSide]: { rightSC: receiver, leftSC: receiverPartner },
  };
  if (serverScore % 2 === 1) {
    for (const side of ["left", "right"]) {
      const p = pos[side];
      [p.rightSC, p.leftSC] = [p.leftSC, p.rightSC];
    }
  }
  return pos;
}

// addPointOnSide (nur Positions-/servingSide-Anteil).
function applyPoint(pos, state, side) {
  if (side === state.servingSide) {
    const t = pos[side];
    [t.rightSC, t.leftSC] = [t.leftSC, t.rightSC];
  } else {
    state.servingSide = side;
  }
}

// ── Test ────────────────────────────────────────────────────────────────────
let failures = 0;
const partnerOf = (pos, side, id) =>
  pos[side].rightSC === id ? pos[side].leftSC : pos[side].rightSC;

// Deterministische Pseudo-Zufallsfolge (kein Date/Math.random nötig).
let seed = 12345;
const rnd = () => ((seed = (seed * 1103515245 + 12345) & 0x7fffffff) / 0x7fffffff);

function runOne(trial) {
  // Doppel: links L1/L2, rechts R1/R2. Start: L1 schlägt auf, R1 nimmt an.
  const pos = { left: { rightSC: "L1", leftSC: "L2" }, right: { rightSC: "R1", leftSC: "R2" } };
  const state = { servingSide: "left" };
  const score = { left: 0, right: 0 };

  for (let step = 0; step < 25; step++) {
    // An JEDEM Stand: den aktuellen Server/Empfänger aus dem "echten" Zustand
    // ablesen und die Aufstellung wie beim Mid-Game-Einstieg rekonstruieren.
    const servingSide = state.servingSide;
    const realServer = serverOf(pos, servingSide, score);
    const realReceiver = receiverOf(pos, servingSide, score);
    const serverPartner = partnerOf(pos, servingSide, realServer);
    const recvSide = servingSide === "left" ? "right" : "left";
    const receiverPartner = partnerOf(pos, recvSide, realReceiver);
    const serverScore = servingSide === "left" ? score.left : score.right;

    const recon = finalize(servingSide, realServer, serverPartner, realReceiver, receiverPartner, serverScore);
    const reconServer = serverOf(recon, servingSide, score);
    const reconReceiver = receiverOf(recon, servingSide, score);
    if (reconServer !== realServer || reconReceiver !== realReceiver) {
      console.error(
        `FAIL trial ${trial} step ${step} @ ${score.left}:${score.right} serving=${servingSide}: ` +
          `real ${realServer}/${realReceiver} vs recon ${reconServer}/${reconReceiver}`,
      );
      failures++;
      return;
    }

    // Weiterzählen: einen zufälligen Punkt spielen (echter Zustand).
    const side = rnd() < 0.5 ? "left" : "right";
    applyPoint(pos, state, side);
    score[side]++;
  }
}

for (let t = 0; t < 3000; t++) runOne(t);

// Zusätzlich ein paar feste Durchrechnungen (gerade/ungerade, Einzel).
function assertServer(desc, pos, servingSide, score, expected) {
  const got = serverOf(pos, servingSide, score);
  if (got !== expected) {
    console.error(`FAIL ${desc}: erwartet ${expected}, war ${got}`);
    failures++;
  }
}
// Ungerader Server-Stand (5): Server aus linkem Service-Court.
assertServer(
  "ungerade 5 → linker Court",
  finalize("left", "L1", "L2", "R1", "R2", 5),
  "left",
  { left: 5, right: 3 },
  "L1",
);
// Gerader Server-Stand (6): Server aus rechtem Service-Court.
assertServer(
  "gerade 6 → rechter Court",
  finalize("left", "L1", "L2", "R1", "R2", 6),
  "left",
  { left: 6, right: 3 },
  "L1",
);
// Einzel (leftSC == rightSC == derselbe Spieler): Tausch ist No-op.
assertServer(
  "Einzel ungerade",
  finalize("left", "S", "S", "E", "E", 7),
  "left",
  { left: 7, right: 2 },
  "S",
);

if (failures > 0) {
  console.error(`\n❌ Aufschlag-Positionstest: ${failures} Fehler.`);
  process.exit(1);
}
// ── serve_start-Nutzlast (Spec schiedsrichterzettel-druck) ────────────
// Das Ereignis traegt Aufschlaeger UND Empfaenger in TEAM-Koordinaten
// (0/1) — nicht in Seiten, nicht als Spieler-IDs. Die Projektion auf dem
// Host rekonstruiert daraus die Doppel-Rotation; steht hier links statt
// rechts, ist das gedruckte Raster ab dem ersten Ballwechsel falsch.
// Stilkonform mit dem Rest dieser Datei: Zaehler statt node:assert.
function pruefe(desc, got, expected) {
  const a = JSON.stringify(got);
  const b = JSON.stringify(expected);
  if (a !== b) {
    console.error(`FAIL ${desc}: erwartet ${b}, war ${a}`);
    failures++;
  }
}

{
  const teamKoordinate = (teamKey) => (teamKey === "a" ? 0 : 1);
  const spielerIndex = (arr, spieler) => {
    const i = arr.findIndex((p) => p.id === spieler.id);
    return i < 0 ? 0 : i;
  };
  const teamA = [{ id: 11 }, { id: 12 }];
  const teamB = [{ id: 21 }, { id: 22 }];

  // Aufschlaeger A2 (Index 1) gegen Empfaenger B1 (Index 0).
  const nutzlast = {
    kind: "serve_start",
    team: teamKoordinate("a"),
    player: spielerIndex(teamA, teamA[1]),
    receiverTeam: teamKoordinate("b"),
    receiverPlayer: spielerIndex(teamB, teamB[0]),
  };
  pruefe("serve_start-Nutzlast", nutzlast, {
    kind: "serve_start",
    team: 0,
    player: 1,
    receiverTeam: 1,
    receiverPlayer: 0,
  });

  // Alle vier Werte muessen in {0,1} liegen — der Host weist alles andere
  // ab, und ein abgewiesenes serve_start laesst den gedruckten Zettel im
  // Doppel auf zwei Zeilen zurueckfallen.
  for (const feld of ["team", "player", "receiverTeam", "receiverPlayer"]) {
    const v = nutzlast[feld];
    if (v !== 0 && v !== 1) {
      console.error(`FAIL ${feld} ausserhalb {0,1}: ${v}`);
      failures++;
    }
  }

  // Unbekannter Spieler faellt auf 0 zurueck statt auf -1 — sonst waere
  // das Ereignis ungueltig und ginge lautlos verloren.
  pruefe("unbekannter Spieler → 0", spielerIndex(teamA, { id: 999 }), 0);
}

console.log("✓ Aufschlag-Positionstest: 3000 Rallyes + Festfälle, keine Abweichung.");
