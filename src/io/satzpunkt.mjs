// Satzpunkt-Ansage im Schiri-Modus des Zähl-Tablets.
//
// Wann gehört das Wort „Satzpunkt" bzw. „Spielpunkt" in die Ansage? Nur in
// dem Moment, in dem eine Mannschaft einen Satzpunkt NEU erreicht — nicht bei
// jedem weiteren Ballwechsel, solange er besteht. DBV-Vokabular, Anhang 4,
// 7.1.1/7.1.2 („Sobald … die führende Seite 10 Punkte erreicht hat, erfolgt
// die Ansage" bzw. „Sobald … eine Seite 14 Punkte erreicht, erfolgt für jede
// Seite die Ansage") und BWF ITTO 5.5.1/5.5.2 („When the leading side
// reaches 20 points …", „If a side reaches 29 points … for each side").
// Schiri-Rückmeldung vom 14.11.2026: Das Tablet sagte bei 20:15, 20:16,
// 20:17 … jedes Mal „Satzpunkt" an.
//
// Daraus folgt für jede Zählweise (21 mit Obergrenze 30, 15 mit 21, 11 mit
// 15 — und was BTP sonst als Ziel/Obergrenze schickt):
// - 20:15 → Satzpunkt; 20:16 … 20:19 → nur der Stand;
// - nach einem Gleichstand in der Verlängerung entsteht ein Satzpunkt neu
//   (21:20, 22:21 …) und wird wieder angesagt;
// - beim letzten Punkt vor der Obergrenze (29:29, 20:20 bei 15er-, 14:14 bei
//   11er-Sätzen) gilt er für beide Mannschaften, der Neue ist der zweite:
//   „29 Satzpunkt beide".
// Verglichen wird je MANNSCHAFT, nicht je Feldseite: Ein Seitenwechsel
// zwischen zwei Ständen darf keinen Satzpunkt erfinden.
//
// `src-tauri/assets/tablet.html` trägt eine Inline-Kopie (kein Modul-Import
// möglich); `scripts/test-inline-kopien.mjs` vergleicht ihr Verhalten mit
// diesem Modul. Deshalb: schlichte `function`-Deklarationen.

/** Gewinnerseite eines Satzstands, sonst `null`. `ziel` ≥ 99 = Zählweise
 *  ohne Satzende (dann nie ein Satzpunkt). Gleiche Regel wie
 *  `setWinnerSide` im Tablet: Obergrenze schlägt den Zwei-Punkte-Abstand. */
export function satzSieger(stand, ziel, cap) {
  if (!ziel || ziel >= 99) return null;
  if (stand.left >= cap) return 'left';
  if (stand.right >= cap) return 'right';
  if (stand.left >= ziel && stand.left - stand.right >= 2) return 'left';
  if (stand.right >= ziel && stand.right - stand.left >= 2) return 'right';
  return null;
}

/** Welche Feldseite gewinnt mit dem nächsten Ballwechsel den Satz?
 *  `{ left, right }`, beide `false` = kein Satzpunkt. */
export function satzpunktSeiten(stand, ziel, cap) {
  return {
    left: satzSieger({ left: stand.left + 1, right: stand.right }, ziel, cap) === 'left',
    right: satzSieger({ left: stand.left, right: stand.right + 1 }, ziel, cap) === 'right',
  };
}

/** Dieselbe Frage je Mannschaft: `{ a, b }`. `teamOnSide` = `{ a: 'left'|'right' }`
 *  wie im Tablet-Zustand; ohne Angabe steht a links. */
export function satzpunktTeams(stand, ziel, cap, teamOnSide) {
  const s = satzpunktSeiten(stand, ziel, cap);
  const aLinks = !teamOnSide || teamOnSide.a !== 'right';
  return aLinks ? { a: s.left, b: s.right } : { a: s.right, b: s.left };
}

/** Gehört das Wort in die Ansage? `jetzt`/`vorher` sind `satzpunktTeams`
 *  des aktuellen und des vorigen Stands im selben Satz. Ist der vorige Stand
 *  unbekannt (kein Undo-Stand, anderer Satz, Einstieg mitten im Spiel), wird
 *  angesagt — lieber einmal zu oft als einen neuen Satzpunkt verschweigen. */
export function satzpunktAnsagen(jetzt, vorher) {
  if (!jetzt.a && !jetzt.b) return false;
  if (!vorher) return true;
  return (jetzt.a && !vorher.a) || (jetzt.b && !vorher.b);
}
