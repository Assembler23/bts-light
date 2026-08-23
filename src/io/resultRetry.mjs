/** Wie geht es weiter, wenn ein Ergebnis nicht durchkommt?
 *  (Feldtest Köpi-Cup, 22.08.2026)
 *
 *  Am Turnier standen Tablets, die weiterzählten und ihre Punkte auch
 *  übertrugen, ihr Spiel aber nicht abschließen konnten. Auf dem Schirm stand
 *  dabei durchgehend „wird automatisch wiederholt, bis es ankommt" — ein
 *  Versprechen, das nie eingelöst wurde: Der Turnier-PC lehnte genau dieses
 *  Ergebnis dauerhaft ab, und keine Wiederholung konnte daran etwas ändern.
 *
 *  Das Tablet warf bis dahin **jede** Absage in denselben Topf: den
 *  Netzwerkfehler, die abgelaufene Verbindung und die inhaltliche Ablehnung.
 *  Diese Datei zieht die Trennlinie, und zwar an genau einer Frage:
 *
 *  > Kann derselbe Payload beim nächsten Versuch angenommen werden?
 *
 *  - **Ja** → weiter wiederholen. Netzfehler, Zeitüberschreitung, HTTP-Fehler,
 *    „kein Match auf diesem Feld", „Feld inzwischen anders belegt": Das hängt
 *    am Netz oder am Zustand des Turnier-PCs, und der ändert sich mit dem
 *    nächsten BTP-Poll.
 *  - **Nein** → aufhören und den Grund zeigen. Der Turnier-PC sagt das
 *    ausdrücklich (`permanent: true`), wenn die Ablehnung allein am Payload
 *    hängt — eine unstimmige Satzliste, ein Satz, der nicht zur Zählweise des
 *    Spiels passt.
 *
 *  **Im Zweifel wird wiederholt.** Ein zu Unrecht abgebrochener Versuch kostet
 *  ein Ergebnis; eine überflüssige Wiederholung kostet einen HTTP-Request.
 *  Deshalb gilt „dauerhaft" nur, wenn der Host es ausdrücklich sagt — ältere
 *  Hosts kennen das Feld nicht, und deren Absagen bleiben wiederholbar.
 *
 *  Kanonische Fassung. `assets/tablet.html` trägt eine Inline-Kopie (die
 *  Assets durchlaufen keinen Build und können keine Module laden) —
 *  Änderungen hier und dort gemeinsam.
 */

/** Standardtext, wenn der Host keinen Grund mitschickt. */
export const OHNE_GRUND = "Der Turnier-PC hat das Ergebnis abgelehnt.";

/**
 * Wie soll das Tablet auf eine Antwort reagieren?
 *
 * @param {object|null} antwort Der geparste Antwort-Body (`{ok, error, permanent}`),
 *   oder `null`/`undefined`, wenn gar keine verwertbare Antwort ankam
 *   (Netzfehler, Abbruch, HTTP-Fehler, kaputtes JSON).
 * @returns {{art: "ok"|"wiederholen"|"dauerhaft", grund: string}}
 *   `art` sagt, was zu tun ist; `grund` ist der anzuzeigende Text (leer bei "ok").
 */
export function naechsterSchritt(antwort) {
  // Keine verwertbare Antwort: Das Ergebnis hat den Host vielleicht nie
  // erreicht — immer wiederholen.
  if (!antwort || typeof antwort !== "object") {
    return { art: "wiederholen", grund: "" };
  }
  if (antwort.ok === true) return { art: "ok", grund: "" };
  const grund = typeof antwort.error === "string" && antwort.error.trim()
    ? antwort.error.trim()
    : OHNE_GRUND;
  // Nur das ausdrückliche Ja des Hosts beendet die Wiederholung.
  return antwort.permanent === true
    ? { art: "dauerhaft", grund }
    : { art: "wiederholen", grund };
}
