// Satzpunkt-Ansage im Schiri-Modus des Zähl-Tablets.
//
// Wann gehört das Wort „Satzpunkt" bzw. „Spielpunkt" in die Ansage? Genau
// EINMAL je Satz — in dem Moment, in dem die führende Seite zum ersten Mal
// einen Punkt vor dem Satzgewinn steht — und danach nur noch am letzten
// Punkt vor der Obergrenze. DBV-Vokabular, Anhang 4, 7.1.1/7.1.2 („Sobald …
// die führende Seite 10 Punkte erreicht hat, erfolgt die Ansage" bzw.
// „Sobald … eine Seite 14 Punkte erreicht, erfolgt für jede Seite die
// Ansage") und BWF ITTO 5.5.1/5.5.2 („When the leading side reaches 20
// points for the first time in each game …", „If a side reaches 29 points
// … for each side").
// Schiri-Rückmeldung vom 14.11.2026: Das Tablet sagte bei 20:15, 20:16,
// 20:17 … jedes Mal „Satzpunkt" an. Rückmeldung vom 15.11.2026: Auch in der
// Verlängerung (21:20, 22:21 …) und beim Wechsel zur anderen Mannschaft
// (20:21) gehört das Wort NICHT noch einmal in die Ansage.
//
// Daraus folgt für jede Zählweise (21 mit Obergrenze 30, 15 mit 21, 11 mit
// 15 — und was BTP sonst als Ziel/Obergrenze schickt):
// - 20:15 → Satzpunkt; 20:16 … 20:19, 21:20, 21:22 … → nur der Stand;
// - erreicht eine Mannschaft den letzten Punkt vor der Obergrenze (29 bei
//   21er-, 20 bei 15er-, 14 bei 11er-Sätzen), wird für sie angesagt:
//   „29 Satzpunkt 28", dann „29 Satzpunkt beide".
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

/** Punktestand je Mannschaft: `{ a, b }`. `teamOnSide` = `{ a: 'left'|'right' }`
 *  wie im Tablet-Zustand; ohne Angabe steht a links. */
export function punkteTeams(stand, teamOnSide) {
  const aLinks = !teamOnSide || teamOnSide.a !== 'right';
  return aLinks ? { a: stand.left, b: stand.right } : { a: stand.right, b: stand.left };
}

/** Gehört das Wort in die Ansage? `jetzt`/`vorher` sind `punkteTeams` des
 *  aktuellen und des vorigen Stands im selben Satz. Angesagt wird, wenn der
 *  Satzpunkt der ERSTE des Satzes ist (vorher stand keine Mannschaft einen
 *  Punkt vor dem Ziel) oder eine Mannschaft soeben den letzten Punkt vor der
 *  Obergrenze erreicht hat. Ist der vorige Stand unbekannt (kein Undo-Stand,
 *  anderer Satz, Einstieg mitten im Spiel), wird angesagt — lieber einmal
 *  zu oft als einen neuen Satzpunkt verschweigen. */
export function satzpunktAnsagen(jetzt, vorher, ziel, cap) {
  const s = satzpunktSeiten({ left: jetzt.a, right: jetzt.b }, ziel, cap);
  if (!s.left && !s.right) return false;
  if (!vorher) return true;
  if (vorher.a < ziel - 1 && vorher.b < ziel - 1) return true;
  return (jetzt.a === cap - 1 && vorher.a < cap - 1) || (jetzt.b === cap - 1 && vorher.b < cap - 1);
}
