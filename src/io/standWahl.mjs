// Wessen Spielstand gilt nach einer (Wieder-)Anmeldung des Zähl-Tablets?
// (Spec tablet-stand-robust, Regel A; ADR 0083 — ergänzt ADR 0017.)
//
// Bisher folgte das Tablet blind dem Bit `authoritative` des Servers. An
// einem FREIEN Feld ist es immer gesetzt — auch für ein Ersatzgerät, das
// noch nie gezählt hat. Es startete bei 0:0 und überschrieb damit den
// gesicherten Stand seines ausgefallenen Vorgängers.
//
// Die Entscheidung fällt jetzt am Tablet, in zwei Stufen, und hängt am
// FORTSCHRITT des Spiels, nicht daran, wer den Slot hält:
//
//   Stufe 1 — `state_restore`: Der Server schickt seinen gesicherten Stand.
//   Stufe 2 — `match_assigned`: Der Server nennt das Spiel und (neu) die
//             Sätze, die er selbst kennt (`hostSets`).
//
// Der Server deutet den Zustand weiter nicht — er bleibt für ihn eine
// undurchsichtige Zeichenkette. Deshalb liegt die Regel hier und nicht in
// `state.rs`/im Relay (dort müsste sie doppelt stehen und hülfe alten
// Seiten trotzdem nicht).
//
// `src-tauri/assets/tablet.html` trägt eine Inline-Kopie (kein Modul-Import
// möglich); `scripts/test-inline-kopien.mjs` vergleicht ihr Verhalten mit
// diesem Modul. Deshalb: schlichte `function`-Deklarationen.

/** So viele selbst gemeldete Satzlisten merkt sich das Tablet. */
export const MELDE_RING_MAX = 8;

/** Fortschritt eines Tablet-Stands: `[abgeschlossene Sätze, Punktsumme
 *  aller Sätze samt laufendem]`. Seitenunabhängig. `finished`, `retired`
 *  und `walkover` zählen bewusst NICHT mit — sonst schlüge ein veralteter
 *  beendeter Stand einen wiedereröffneten. Unlesbares ergibt `[0, 0]`. */
export function fortschritt(stand) {
  var saetze = stand && Array.isArray(stand.setsCompleted) ? stand.setsCompleted : [];
  var summe = 0;
  for (var i = 0; i < saetze.length; i++) summe += punkte(saetze[i], "left", "right");
  if (stand && stand.currentSet) summe += punkte(stand.currentSet, "left", "right");
  return [saetze.length, summe];
}

/** Summe zweier Zahlenfelder eines Satzes; alles andere zählt 0. */
export function punkte(satz, x, y) {
  if (!satz || typeof satz !== "object") return 0;
  var a = typeof satz[x] === "number" && satz[x] > 0 ? satz[x] : 0;
  var b = typeof satz[y] === "number" && satz[y] > 0 ? satz[y] : 0;
  return a + b;
}

/** Lexikografischer Vergleich zweier Fortschritte: −1, 0, +1. */
export function vergleiche(f, g) {
  if (f[0] !== g[0]) return f[0] < g[0] ? -1 : 1;
  if (f[1] !== g[1]) return f[1] < g[1] ? -1 : 1;
  return 0;
}

/** Ist dieser Satz (Team-Koordinaten `a`/`b`) zu Ende gespielt? Dieselbe
 *  Regel wie `setWinnerSide` der Seite: Deckel erreicht, oder Ziel erreicht
 *  mit zwei Punkten Abstand. Zeit-Formate (Ziel ≥ 99) enden nie von selbst. */
export function satzEntschieden(satz, ziel, deckel) {
  if (!(ziel > 0) || ziel >= 99) return false;
  // Fehlt der Deckel (alter Server), dieselbe Ableitung wie `capForTarget`.
  var cap = deckel >= ziel ? deckel
    : ziel === 21 ? 30 : ziel === 15 ? 21 : ziel === 11 ? 15 : ziel + 5;
  var a = satz.a, b = satz.b;
  if (a >= cap || b >= cap) return true;
  return (a >= ziel && a - b >= 2) || (b >= ziel && b - a >= 2);
}

/** Bringt die Satzliste des Servers in Form: nur Zahlenpaare ≥ 0, höchstens
 *  neun, ein abschließender 0:0-Satz fällt weg. Unlesbares ergibt `[]`. */
export function hostSaetzeLesen(sets) {
  if (!Array.isArray(sets) || sets.length > 9) return [];
  var aus = [];
  for (var i = 0; i < sets.length; i++) {
    var s = sets[i];
    if (!s || typeof s.a !== "number" || typeof s.b !== "number") return [];
    if (!(s.a >= 0) || !(s.b >= 0) || s.a % 1 !== 0 || s.b % 1 !== 0 || s.a > 99 || s.b > 99) return [];
    aus.push({ a: s.a, b: s.b });
  }
  while (aus.length && aus[aus.length - 1].a === 0 && aus[aus.length - 1].b === 0) aus.pop();
  return aus;
}

/** Trennt die Satzliste des Servers in abgeschlossene Sätze und den
 *  laufenden. Der Server führt „abgeschlossene + laufender" als EINE Liste;
 *  ob der letzte Eintrag noch läuft, sagt die Zählweise des Spiels.
 *  `entschieden` = die abgeschlossenen Sätze entscheiden das Spiel schon. */
export function hostSaetzeTeilen(sets, ziel, deckel, bestOf) {
  var liste = hostSaetzeLesen(sets);
  var fertig = liste.slice();
  var laufend = { a: 0, b: 0 };
  if (liste.length && !satzEntschieden(liste[liste.length - 1], ziel, deckel)) {
    laufend = fertig.pop();
  }
  var a = 0, b = 0, summe = laufend.a + laufend.b;
  for (var i = 0; i < fertig.length; i++) {
    if (fertig[i].a > fertig[i].b) a++; else if (fertig[i].b > fertig[i].a) b++;
    summe += fertig[i].a + fertig[i].b;
  }
  var noetig = Math.ceil((bestOf > 0 ? bestOf : 3) / 2);
  return {
    fertig: fertig,
    laufend: laufend,
    entschieden: a >= noetig || b >= noetig,
    fortschritt: [fertig.length, summe],
  };
}

/** Schlüssel einer Satzliste für den Melde-Ring: „21:10,5:3". Ein
 *  abschließender 0:0-Satz zählt nicht (der Server lässt ihn je nach Lage
 *  weg oder stehen). */
export function saetzeSchluessel(sets) {
  var liste = hostSaetzeLesen(sets);
  var teile = [];
  for (var i = 0; i < liste.length; i++) teile.push(liste[i].a + ":" + liste[i].b);
  return teile.join(",");
}

/** Nimmt eine selbst gemeldete Satzliste in den Ring auf (neuer Ring).
 *  Der Ring gehört zu EINEM Spiel; ein anderes Spiel beginnt ihn neu. */
export function ringMerken(ring, matchId, sets) {
  var k = saetzeSchluessel(sets);
  var alt = ring && ring.matchId === matchId && Array.isArray(ring.saetze) ? ring.saetze : [];
  if (alt.length && alt[alt.length - 1] === k) return { matchId: matchId, saetze: alt };
  var neu = alt.concat([k]);
  return { matchId: matchId, saetze: neu.slice(-MELDE_RING_MAX) };
}

/** Hat DIESES Tablet genau diese Satzliste für dieses Spiel gemeldet? Dann
 *  ist der Stand des Servers nur das Echo eines eigenen, älteren Stands
 *  (der Bediener hat seither zurückgenommen) — und kein fremder Fortschritt. */
export function imRing(ring, matchId, sets) {
  if (!ring || ring.matchId !== matchId || !Array.isArray(ring.saetze)) return false;
  return ring.saetze.indexOf(saetzeSchluessel(sets)) >= 0;
}

/** Stufe 1 — beim `state_restore`.
 *
 *  @param {object} e
 *  @param {boolean} e.ownershipActive  Server im Ownership-Modus?
 *  @param {boolean} e.authoritative    Bit des Servers (Slot-Halter).
 *  @param {null|{matchId:number, rev:number, fortschritt:number[]}} e.lokal
 *         Stand dieses Tablets, `null` ohne Spiel.
 *  @param {{matchId:number|null, rev:number, schreiber:string, fortschritt:number[]}} e.kandidat
 *         Der Stand des Servers.
 *  @param {string} e.eigeneKennung     `schreiber` dieses Tablets.
 *  @returns {"legacy"|"adoptieren"|"lokal"|"vormerken"}
 */
export function wahlBeiRestore(e) {
  // Legacy-Schalter oder alter Server: die alte rev-Logik, wortgleich.
  if (!e.ownershipActive) return "legacy";
  // 1. Übernahme oder finalisiert — der Server hat entschieden.
  if (!e.authoritative) return "adoptieren";
  // 2. Kein eigener Stand → nichts, was man durchsetzen könnte (Kernfix).
  if (!e.lokal) return "adoptieren";
  // 3. Anderes Spiel → erst `match_assigned` sagt, welches gilt.
  if (e.lokal.matchId !== e.kandidat.matchId) return "vormerken";
  // 4. Eigener Stand kommt zurück → die Revision ordnet (Undo, „Wieder
  //    öffnen": der jüngere eigene Stand gilt, auch wenn er weniger zeigt).
  if (e.kandidat.schreiber && e.kandidat.schreiber === e.eigeneKennung) {
    return e.lokal.rev >= e.kandidat.rev ? "lokal" : "adoptieren";
  }
  // 5. Fremder (oder unbekannter) Schreiber → wer weiter ist, gilt.
  return vergleiche(e.kandidat.fortschritt, e.lokal.fortschritt) > 0 ? "adoptieren" : "lokal";
}

/** Stufe 2 — beim `match_assigned` für ein nicht finalisiertes Spiel.
 *
 *  @param {object} e
 *  @param {number} e.matchId               Das zugewiesene Spiel.
 *  @param {number|null} e.lokalMatchId     Spiel, das das Tablet geladen hat.
 *  @param {number[]} e.lokalFortschritt    Dessen Fortschritt.
 *  @param {null|{matchId:number|null}} e.vorgemerkt  Stand aus Stufe 1.
 *  @param {Array} e.hostSets               Satzliste des Servers (roh).
 *  @param {number[]} e.hostFortschritt     Deren Fortschritt.
 *  @param {object} e.ring                  Melde-Ring dieses Tablets.
 *  @returns {"adoptieren"|"neu"|"einstieg"|"behalten"}
 */
export function wahlBeiZuweisung(e) {
  // Der vorgemerkte Stand gehört genau zu diesem Spiel → übernehmen.
  if (e.vorgemerkt && e.vorgemerkt.matchId === e.matchId) return "adoptieren";
  var leer = hostSaetzeLesen(e.hostSets).length === 0;
  if (e.lokalMatchId !== e.matchId) {
    // Kein Stand zu diesem Spiel: Kennt der Server Sätze, wird dort
    // eingestiegen — nie bei 0:0.
    return leer ? "neu" : "einstieg";
  }
  // Der Server ist weiter, und es ist nicht das Echo eines eigenen Stands.
  if (!leer && vergleiche(e.hostFortschritt, e.lokalFortschritt) > 0
      && !imRing(e.ring, e.matchId, e.hostSets)) {
    return "einstieg";
  }
  return "behalten";
}
