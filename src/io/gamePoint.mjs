// Satz-/Matchball-Erkennung für die Felderübersicht (Plan 16). Reine Logik,
// in Node testbar (scripts/test-gamepoint.mjs). Portiert aus der Tablet-
// Näherung `umpPointBadge` (tablet.html): der Führende ist einen Punkt vom
// Satzgewinn entfernt → Satzball; würde damit der entscheidende Satz fallen →
// Matchball. Nur eine Turnierleitungs-Planungshilfe, keine Wertungslogik.

/** Ist der laufende Satz bereits entschieden (Zielpunkt mit 2 Vorsprung oder
 *  Cap erreicht)? Dann ist es kein „Ball" mehr. */
export function setDecided(a, b, target, cap) {
  const hi = Math.max(a, b),
    lo = Math.min(a, b);
  if (cap > 0 && hi >= cap) return true;
  return hi >= target && hi - lo >= 2;
}

/** Wie viele Gewinnsätze bis zum Matchsieg (Best-of → Mehrheit). */
export function setsToWin(bestOf) {
  const bo = bestOf && bestOf > 0 ? bestOf : 3;
  return Math.floor(bo / 2) + 1;
}

/**
 * Liefert `"match"`, `"set"` oder `null` für ein Feld.
 * @param {{sets:[number,number][], best_of:number, target_score:number, cap_score:number, match_id:number}} c
 */
export function gamePointKind(c) {
  const target = c && c.target_score;
  if (!c || c.match_id <= 0 || !target || target >= 99) return null;
  const sets = Array.isArray(c.sets) ? c.sets : [];
  if (sets.length === 0) return null;
  const last = sets[sets.length - 1] || [0, 0];
  const l = last[0] || 0,
    r = last[1] || 0;
  const cap = c.cap_score || 0;
  // Aktueller Satz schon entschieden → kein Ball (verhindert „Matchball" auf
  // einem gerade gewonnenen/beendeten Satz).
  if (setDecided(l, r, target, cap)) return null;
  const lead = Math.max(l, r),
    trail = Math.min(l, r);
  // Ball, wenn der Führende einen Punkt vom Satzgewinn entfernt ist ODER der
  // Stand direkt vor dem Cap steht (dann entscheidet der nächste Punkt den
  // Satz für den Punktgewinner — auch bei Gleichstand, z. B. 29:29 bei Cap 30).
  const capBall = cap > 0 && lead === cap - 1;
  const normalBall = lead - trail >= 1 && lead >= target - 1;
  if (!capBall && !normalBall) return null;
  const need = setsToWin(c.best_of);
  const completed = sets.slice(0, -1);
  const leftWins = completed.filter((s) => (s[0] || 0) > (s[1] || 0)).length;
  const rightWins = completed.filter((s) => (s[1] || 0) > (s[0] || 0)).length;
  // Bei Gleichstand (Cap-Patt) können BEIDE Seiten den Satz mit dem nächsten
  // Punkt gewinnen → Matchball, sobald EINE Seite damit das Match gewinnt.
  if (l === r) {
    return leftWins === need - 1 || rightWins === need - 1 ? "match" : "set";
  }
  const leaderWins = l > r ? leftWins : rightWins;
  return leaderWins === need - 1 ? "match" : "set";
}
