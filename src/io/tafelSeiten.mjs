/** Seitenlogik der Zähltafel (Spec `docs/features/zaehltafel-anzeige-huelle.md`).
 *
 *  Aus `match.sets` (Team-Koordinaten a/b wie vom Host) und dem vom Tablet
 *  gespiegelten `courtState` wird, was links und rechts auf der Tafel steht:
 *  Punkte des laufenden Satzes, gewonnene Sätze, Aufschlag-Punkt.
 *
 *  Regeln (aus dem Grill, 04.09.2026):
 *  - Seiten aus `courtState.teamOnSide` (`a: "left"|"right"`). Fehlt es (vor
 *    der Seitenwahl, oder gar kein zählendes Tablet), steht **Team 1 links**.
 *  - Aufschlag aus `serving.team`; sonst Rückfall auf `servingSide` +
 *    `teamOnSide` (wie monitor.html). Ohne beides: kein Punkt. Nach der
 *    Entscheidung nie ein Punkt.
 *  - `finished`: ein nachgestellter 0:0-Geistersatz fällt weg, der letzte
 *    gespielte Satz steht groß **und** zählt im Satzstand.
 *  - `retired`: der unvollständige letzte Satz steht groß, zählt aber nicht.
 *  - `spiegel` tauscht links/rechts **nach** der Seitenbestimmung — wirkt
 *    also auch ohne `teamOnSide` (Tafel steht dem Schiedsrichter gegenüber).
 *
 *  Kanonische Fassung. `tafel.html` trägt eine Inline-Kopie (die Assets
 *  durchlaufen keinen Build) — Änderungen hier und dort gemeinsam.
 */

function zahl(v) {
  const n = Number(v);
  return Number.isFinite(n) && n > 0 ? Math.floor(n) : 0;
}

/**
 * @param {Array<{a:number,b:number}>|undefined} sets Sätze in Team-Koordinaten.
 * @param {object|null|undefined} courtState Geparster Tablet-Zustand oder null.
 * @param {boolean|undefined} spiegel Links/rechts tauschen.
 * @returns {{links:{punkte:number,saetze:number,aufschlag:boolean},
 *            rechts:{punkte:number,saetze:number,aufschlag:boolean},
 *            entschieden:boolean}}
 */
export function tafelSeiten(sets, courtState, spiegel) {
  const cs = courtState && typeof courtState === "object" ? courtState : null;
  const finished = !!(cs && cs.finished);
  const retired = !!(cs && cs.retired);
  const roh = Array.isArray(sets)
    ? sets.map((s) => ({ a: zahl(s && s.a), b: zahl(s && s.b) }))
    : [];

  // Geistersatz nur im entschiedenen Spiel streichen — im laufenden ist
  // 0:0 der echte Beginn eines Satzes.
  const gespielt = roh.slice();
  if (finished) {
    while (gespielt.length > 1) {
      const l = gespielt[gespielt.length - 1];
      if (l.a === 0 && l.b === 0) gespielt.pop();
      else break;
    }
  }
  const gross = gespielt.length > 0 ? gespielt[gespielt.length - 1] : { a: 0, b: 0 };

  // Satzstand: fertige Sätze zählen. Laufend → alle außer dem letzten;
  // entschieden → alle; Aufgabe → alle außer dem unvollständigen letzten.
  const zaehlbar = finished && !retired ? gespielt : gespielt.slice(0, -1);
  let sA = 0, sB = 0;
  for (const s of zaehlbar) {
    if (s.a > s.b) sA++;
    else if (s.b > s.a) sB++;
  }

  // Aufschlag (nur im laufenden Spiel): "a"|"b"|null.
  let aufschlag = null;
  if (!finished && cs) {
    if (cs.serving && (cs.serving.team === "a" || cs.serving.team === "b") && cs.teamOnSide) {
      aufschlag = cs.serving.team;
    } else if (cs.servingSide && cs.teamOnSide && cs.teamOnSide.a) {
      aufschlag = cs.teamOnSide.a === cs.servingSide ? "a" : "b";
    }
  }

  const teamA = { punkte: gross.a, saetze: sA, aufschlag: aufschlag === "a" };
  const teamB = { punkte: gross.b, saetze: sB, aufschlag: aufschlag === "b" };

  // Seiten: Team 1 links, außer das Tablet sagt, dass A rechts steht.
  const aRechts = !!(cs && cs.teamOnSide && cs.teamOnSide.a === "right");
  let links = aRechts ? teamB : teamA;
  let rechts = aRechts ? teamA : teamB;
  if (spiegel) { const t = links; links = rechts; rechts = t; }

  return { links, rechts, entschieden: finished };
}
