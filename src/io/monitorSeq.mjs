/** Ordnung zwischen Push und Voll-Abruf auf den Anzeige-Seiten
 *  (Spec `docs/features/monitor-livestand-push.md`, Etappe S4).
 *
 *  Jeder Feld-Stand trägt eine Ordnungszahl `seq`: Der Nudge auf der
 *  Monitor-WS nennt sie, und seit v0.9.239 nennen sie auch die Voll-Antworten
 *  (`/health`, `/court/{id}/state`). Damit lässt sich entscheiden, ob eine
 *  eintreffende Nachricht neuer ist als das, was gerade auf dem Schirm steht.
 *
 *  **Die beiden Quellen werden bewusst unterschiedlich behandelt:**
 *
 *  - **Push** (`"push"`): nur bei `seq > gezeigt`. Ein doppelter oder
 *    verzögerter Nudge trägt dieselbe oder eine kleinere Zahl und ist
 *    wertlos — er löste nur einen überflüssigen Abruf aus.
 *  - **Voll-Abruf** (`"fetch"`): schon bei `seq >= gezeigt`. Das
 *    Gleichheitszeichen ist Absicht: Die Voll-Antwort trägt denselben Stand,
 *    den der Nudge angekündigt hat — und sie darf ihn auch dann noch
 *    berichtigen, wenn sich am Zähler nichts geändert hat. Genau das
 *    passiert, wenn BTP einen Satzstand von Hand zurücknimmt: Der Inhalt
 *    ändert sich, ohne dass ein Nudge dazwischenliegt.
 *
 *  `seq === 0` heißt „keine Ordnung bekannt" — ein älterer Server, der das
 *  Feld nicht schickt. Dann gilt der Stand immer, und die Seite verhält sich
 *  wie vor dieser Etappe. Dasselbe für einen unbekannten Quellen-Namen:
 *  lieber anzeigen als eine Anzeige einfrieren.
 *
 *  Kanonische Fassung. `overview.html` und `monitor.html` tragen eine
 *  Inline-Kopie (die Assets durchlaufen keinen Build und können keine Module
 *  laden) — Änderungen hier und dort gemeinsam.
 */

/**
 * Soll ein eintreffender Stand angewendet werden?
 *
 * @param {number} gezeigtSeq Ordnungszahl des aktuell gezeigten Stands (0 = noch keiner).
 * @param {number} seq Ordnungszahl des eintreffenden Stands (0 = unbekannt).
 * @param {"push"|"fetch"} quelle Woher er kommt.
 * @returns {boolean}
 */
export function anwenden(gezeigtSeq, seq, quelle) {
  const gezeigt = Number.isFinite(gezeigtSeq) ? gezeigtSeq : 0;
  const neu = Number.isFinite(seq) ? seq : 0;
  // Ohne Ordnung (alter Server) nie blockieren.
  if (neu <= 0) return true;
  if (gezeigt <= 0) return true;
  // Push muss echt neuer sein; ein Voll-Abruf darf gleichziehen.
  return quelle === "push" ? neu > gezeigt : neu >= gezeigt;
}
