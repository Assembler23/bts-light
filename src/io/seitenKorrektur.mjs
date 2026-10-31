/**
 * Seiten und Mannschaften am Zähl-Tablet von Hand korrigieren.
 *
 * Kanonische Fassung (Spec `docs/features/tablet-seiten-korrigieren.md`,
 * ADR 0067). `src-tauri/assets/tablet.html` trägt eine **Inline-Kopie**
 * (die Assets durchlaufen keinen Build und können keine Module laden) —
 * Änderungen hier und dort gemeinsam.
 *
 * Alle Funktionen sind rein: Sie bekommen den Zustand als schlichtes Objekt
 * (Felder wie `STATE` am Tablet) und liefern die geänderten Felder neu, ohne
 * die Eingabe anzufassen. So ist jede Korrektur ohne Browser prüfbar —
 * ein falscher Tausch fiele sonst erst beim gemeldeten Ergebnis auf.
 */

/** Höchstzahl Ereignisse je Match — gleich dem Deckel des Hosts. */
export const MAX_EVENTS = 64;

const kippeSeite = (s) => (s === "left" ? "right" : "left");
const kippeTeam = (t) => (t === "a" ? "b" : t === "b" ? "a" : t);

/**
 * „Seiten tauschen": Die Spieler haben die Hälften gewechselt. Mannschaften,
 * Punkte, Satzstände und Aufschlag wandern **gemeinsam** — der gemeldete
 * Stand A:B bleibt gleich. Dieselbe Spiegelung wie der automatische
 * Seitenwechsel nach dem Satz und bei 11 im Entscheidungssatz.
 */
export function seitenGetauscht(z) {
  if (!z.teamOnSide || !z.positions) return null;
  return {
    teamOnSide: { a: kippeSeite(z.teamOnSide.a), b: kippeSeite(z.teamOnSide.b) },
    positions: { left: z.positions.right, right: z.positions.left },
    currentSet: { left: z.currentSet.right, right: z.currentSet.left },
    setsCompleted: z.setsCompleted.map((s) => ({ left: s.right, right: s.left })),
    servingSide: kippeSeite(z.servingSide ?? "left"),
  };
}

/** Spieler-ID der anderen Mannschaft an derselben Listenposition. */
function gegenstueck(match, id) {
  for (const [von, nach] of [
    ["teamA", "teamB"],
    ["teamB", "teamA"],
  ]) {
    const i = (match[von] || []).findIndex((p) => p.id === id);
    if (i >= 0) {
      const ziel = match[nach] || [];
      const p = ziel[i] ?? ziel[0];
      return p ? p.id : id;
    }
  }
  return id;
}

/**
 * „Mannschaften tauschen": Am Tablet stand die falsche Mannschaft auf der
 * Seite. Die Punkte gehören zur **Seite** (der Zähler hat richtig getippt),
 * nur die Namen wechseln — gemeldet wird danach B:A statt A:B.
 *
 * - `positions` hält Spieler-IDs: im Einzel eindeutig umgemappt, im Doppel
 *   nach Listenposition als Platzhalter, bis das Tablet die Aufstellung neu
 *   abgefragt hat.
 * - Der Punktverlauf steht in A/B und kippt mit.
 * - `serveSetupTeam` (anstehende Satzbeginn-Abfrage) und lokale Karten
 *   kippen mit; Karte und Strafpunkt bleiben auf ihrer Seite.
 * - Der Sieger einer Aufgabe/eines Kampflos-Spiels wird neu gewählt.
 */
export function mannschaftenGetauscht(z) {
  if (!z.teamOnSide || !z.match) return null;
  const pos = z.positions;
  const umgemappt = (p) =>
    p && {
      rightSC: gegenstueck(z.match, p.rightSC),
      leftSC: gegenstueck(z.match, p.leftSC),
    };
  const log = z.rallyLog;
  const aus = (team) => (team === "a" ? "teamA" : "teamB");
  return {
    teamOnSide: { a: z.teamOnSide.b, b: z.teamOnSide.a },
    positions: pos ? { left: umgemappt(pos.left), right: umgemappt(pos.right) } : pos,
    rallyLog: log
      ? {
          ...log,
          sets: log.sets.map((s) => ({
            startA: s.startB,
            startB: s.startA,
            points: s.points.replace(/[AB]/g, (c) => (c === "A" ? "B" : "A")),
          })),
        }
      : log,
    serveSetupTeam: kippeTeam(z.serveSetupTeam ?? null),
    cards: (z.cards || []).map((c) => {
      const team = kippeTeam(c.team);
      const liste = z.match[aus(team)] || [];
      const p = liste[c.index] ?? liste[0];
      return { ...c, team, name: p ? p.name : c.name };
    }),
    retiredWinner: null,
  };
}

/** Gemeldeter Stand in Team-Koordinaten — wie `sendScoreUpdate`. */
export function standAB(z) {
  if (!z.teamOnSide) return null;
  const ab = (s) => (z.teamOnSide.a === "left" ? { a: s.left, b: s.right } : { a: s.right, b: s.left });
  return { saetze: z.setsCompleted.map(ab), aktuell: ab(z.currentSet) };
}

/**
 * Darf korrigiert werden? Im laufenden Spiel (auch in Pausen) und in der
 * Ergebnis-Ansicht, solange das Ergebnis nicht abgeschickt ist. Nie am
 * gesperrten oder passiven Tablet und nie während der Aufstellung — auch
 * nicht, solange nach „Mannschaften tauschen" die Neuabfrage der
 * Aufstellung aussteht (sonst stapelten sich zwei Korrekturen um eine
 * offene Abfrage, und das Zurücknehmen der oberen verlöre sie).
 */
export function korrekturErlaubt(l) {
  return (
    !!l.match &&
    !!l.teamOnSide &&
    l.setup === "playing" &&
    !l.berichtigtAbfrage &&
    !l.pendingResult &&
    !l.submittedOk &&
    !l.finalized &&
    !l.locked &&
    !l.occupied
  );
}

/**
 * Anker des Ereignisses „Mannschaften berichtigt". Im laufenden Spiel der
 * übliche `(Satz, Ballwechsel)`; im beendeten Spiel der **letzte gespielte**
 * Satz — der übliche Anker zeigte dort auf Satz n+1, und den verwürfen Host
 * und Relay im fünften Satz als ungültig.
 */
export function korrekturAnker(z) {
  const fertig = z.setsCompleted ? z.setsCompleted.length : 0;
  const satz = z.finished ? Math.max(1, fertig) : fertig + 1;
  const s = z.rallyLog && z.rallyLog.sets ? z.rallyLog.sets[satz - 1] : null;
  let anker = { set: satz, afterN: s && s.points ? s.points.length : 0 };
  if (!z.finished) return anker;
  // Beendet: nie VOR einem gültigen Ereignis. Eine Karte aus der Satzpause
  // steht schon am nächsten Satz (n+1, 0); endet das Spiel danach per
  // Aufgabe, bliebe sie sonst ungekippt. Im laufenden Spiel liegt kein
  // gültiges Ereignis hinter dem Stand (Undo nimmt sie zurück), und
  // zurückgenommene dürfen den Anker nicht verschieben — sonst nähme das
  // nächste Undo die Berichtigung mit.
  const events = z.events || [];
  const zurueck = new Set(events.filter((e) => e.kind === "retract").map((e) => e.retracts));
  for (const e of events) {
    if (e.kind === "retract" || zurueck.has(e.id)) continue;
    const set = e.set || 0;
    const afterN = e.afterN || 0;
    if (set > anker.set || (set === anker.set && afterN > anker.afterN)) anker = { set, afterN };
  }
  return anker;
}

/**
 * Reicht der Ereignis-Deckel? Gebraucht werden die Berichtigung und ihre
 * mögliche Rücknahme, im Doppel zusätzlich die neue Aufschlagfolge samt
 * Rücknahme. Lieber verweigern als ohne Ereignis kippen — dann stünde der
 * Zettel still falsch.
 */
export function platzFuerKorrektur(events, doppel) {
  return (events || []).length + (doppel ? 4 : 2) <= MAX_EVENTS;
}

/**
 * Muss das Tablet nach „Mannschaften tauschen" die Aufstellung neu
 * abfragen? Nur im Doppel, nur im laufenden Spiel, und nicht, wenn die
 * Satzbeginn-Abfrage ohnehin ansteht.
 */
export function brauchtAbfrage(z, doppel) {
  return !!doppel && !z.finished && !z.serveSetupTeam;
}
