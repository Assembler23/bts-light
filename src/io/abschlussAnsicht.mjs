/** Was zeigt die Beendet-Ansicht des Zähl-Tablets, und welche Knöpfe gehen?
 *  (Turnier 12./13.09.2026, Feld 11)
 *
 *  Das Ergebnis war gesendet, BTP hatte es angenommen und das Spiel
 *  finalisiert, das Tablet war zurückgetreten (ADR 0017, Regel b: Ein in BTP
 *  festes Ergebnis wird vom Tablet nie mehr überbügelt). Trotzdem stand der
 *  Knopf „Korrektur — Match wieder öffnen" weiter offen. Der Bediener drückte
 *  ihn, zählte um, und drückte dann 23-mal „Ergebnis übermitteln" — jedes Mal
 *  schluckte das Finalisiert-Gate den Versuch **still**. Auf dem Schirm stand
 *  nichts; im Log stand `submit_suppressed_finalized`, 23-mal.
 *
 *  Das Gate ist richtig. Falsch war, dass die Ansicht es nicht zeigte: Ein
 *  Knopf, der nichts tut und nichts sagt, ist schlimmer als keiner. Diese
 *  Regel entscheidet deshalb an EINER Stelle, was die Beendet-Ansicht sperrt
 *  und was sie sagt. Ist das Ergebnis in BTP fest, sind **beide** Knöpfe zu,
 *  und der Text nennt den einzigen Weg, der noch geht — die Turnierleitung.
 *
 *  Alle anderen Lagen bleiben, wie sie vorher waren; die Texte sind die alten.
 *
 *  Kanonische Fassung. `assets/tablet.html` trägt eine Inline-Kopie (die
 *  Assets durchlaufen keinen Build und können keine Module laden) —
 *  Änderungen hier und dort gemeinsam.
 */

/**
 * Ist ein offener Sendeauftrag erledigt, weil BTP das Match festgemacht hat?
 *
 * Trifft das Finalisiert-Frame ein, während `/result` noch unterwegs oder im
 * Retry ist, hat BTP ein Ergebnis — das eigene (die Antwort ging verloren)
 * oder ein von Hand eingetragenes. In beiden Fällen nähme der Turnier-PC den
 * Payload nicht mehr an (R5). Der Auftrag darf dann nicht stehen bleiben:
 * Er blockte sonst beim nächsten Match still den Sende-Knopf und würde nach
 * einem Reload sogar nachgesendet — und am falschen Spiel abgewiesen
 * (Review-Fund 15.09.2026).
 *
 * Nur für DASSELBE Match: Ein Auftrag für ein anderes Spiel bleibt, denn
 * über den sagt das Frame nichts.
 *
 * @param {{matchId?: number}|null} pendingResult Offener Auftrag oder null.
 * @param {number|null|undefined} matchId Match, das der Host gerade festmacht.
 * @param {boolean} finalized Ist es in BTP fest?
 */
export function sendeauftragErledigt(pendingResult, matchId, finalized) {
  return !!(finalized && pendingResult && matchId != null
    && pendingResult.matchId === matchId);
}

/** Text, wenn das Ergebnis in BTP fest ist. */
export const FEST_IN_BTP =
  "✓ Ergebnis steht in BTP fest — Korrektur nur über die Turnierleitung.";

/**
 * Sperren und Statuszeile der Beendet-Ansicht.
 *
 * @param {object} z
 * @param {boolean} z.finalized   Match ist in BTP finalisiert (Frame vom Host).
 * @param {boolean} z.submittedOk Ergebnis wurde vom Turnier-PC angenommen.
 * @param {string|null} z.resultRejected Dauerhafte Ablehnung mit Grund.
 * @param {boolean} z.pendingResult Ergebnis ist unterwegs / wird wiederholt.
 * @param {boolean} z.needWinner  Aufgabe/Kampflos ohne gewählten Sieger.
 * @returns {{sendenGesperrt: boolean, korrekturGesperrt: boolean, status: string}}
 */
export function abschlussLage(z) {
  const s = z || {};
  // Fest in BTP gewinnt gegen alles: Was auch immer das Tablet noch vorhat,
  // der Host nimmt es nicht mehr an — also gar nicht erst anbieten.
  if (s.finalized) {
    return { sendenGesperrt: true, korrekturGesperrt: true, status: FEST_IN_BTP };
  }
  if (s.submittedOk) {
    return {
      sendenGesperrt: true, korrekturGesperrt: false,
      status: "✓ Übermittelt — die Turnierleitung kann übernehmen.",
    };
  }
  if (s.resultRejected) {
    // Dauerhaft abgelehnt: den Grund nennen und den Knopf wieder freigeben.
    // Der Bediener kann den Stand korrigieren und neu senden oder die
    // Turnierleitung holen — beides ist besser als ein Tablet, das
    // stillschweigend endlos wiederholt.
    return {
      sendenGesperrt: false, korrekturGesperrt: false,
      status: "✗ Nicht angenommen: " + s.resultRejected
        + " — bitte den Stand prüfen oder die Turnierleitung ansprechen.",
    };
  }
  if (s.pendingResult) {
    return {
      sendenGesperrt: true, korrekturGesperrt: false,
      status: "Ergebnis wird übermittelt … wird automatisch wiederholt, bis es ankommt.",
    };
  }
  return {
    sendenGesperrt: !!s.needWinner, korrekturGesperrt: false,
    status: s.needWinner ? "Bitte zuerst den Sieger wählen." : "",
  };
}
