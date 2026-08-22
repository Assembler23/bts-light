/** Wann steht eine Anzeige still, ohne es zu merken?
 *  (Feldtest Köpi-Cup, 22.08.2026)
 *
 *  Court-Monitore froren gelegentlich auf einem alten Stand ein — die Seite
 *  lief weiter, rendert weiter, übernahm nur nichts Neues mehr. Das Tückische
 *  daran: Dieses Symptom hinterließ **keine Spur**. Der Log-Upload der
 *  Anzeige-Seiten hängt an JS-Fehler, `unhandledrejection` und `pagehide`; ein
 *  stiller Hänger fällt durch alle drei Raster, und am Ende stand nur „hängt
 *  öfters mal" ohne jede Möglichkeit, die Ursache einzugrenzen.
 *
 *  Dieser Wächter macht den Zustand bezeugbar. Er heilt nichts — er stellt
 *  fest und lädt das Log hoch, damit beim nächsten Mal Zahlen dastehen statt
 *  eines Eindrucks.
 *
 *  **Zwei Größen genügen dafür:**
 *
 *  - `letzterAbrufOkMs` — wann zuletzt ein Abruf ohne Fehler zurückkam.
 *  - `letzterStandMs` — wann zuletzt ein Stand tatsächlich **angewendet oder
 *    als aktuell bestätigt** wurde.
 *
 *  Die Bestätigung gehört ausdrücklich dazu: Die Feld-Übersicht bekommt auf
 *  einen unveränderten Stand ein `304` (Spec monitor-livestand-push, S8). Da
 *  wird nichts gerendert, und trotzdem ist alles in Ordnung — der Server sagt
 *  ja gerade, dass das Gezeigte stimmt. Zählte man das nicht mit, meldete eine
 *  ruhige Halle im Minutentakt Fehlalarme.
 *
 *  Daraus ergeben sich zwei unterscheidbare Lagen:
 *
 *  - `"keine_abrufe"` — es kommt gar nichts mehr zurück. Der Poll-Timer ist
 *    tot, oder der Server/das Netz ist weg.
 *  - `"verworfen"` — die Abrufe klappen, aber nichts davon landet auf dem
 *    Schirm. Dann liegt es an der Seite selbst: der `seq`-Guard verwirft
 *    (`monitor.html` lässt ihn auch auf den Voll-Abruf los), oder das Rendern
 *    kommt nicht durch.
 *
 *  Genau diese Unterscheidung ist der Zweck des Ganzen — sie trennt „Gerät
 *  oder Netz" von „Seite", und das war bisher am hängenden Monitor nicht
 *  feststellbar.
 *
 *  **Ein Fehlalarm ist teuer.** Er schickt Log-Uploads von zwanzig Geräten los
 *  und lässt eine gesunde Halle krank aussehen. Deshalb ist die Schwelle
 *  großzügig, und alles Unklare (fehlende Werte, Zeitstempel aus der Zukunft
 *  nach einem Uhrsprung) gilt als gesund.
 *
 *  Kanonische Fassung. `overview.html` und `monitor.html` tragen eine
 *  Inline-Kopie (die Assets durchlaufen keinen Build und können keine Module
 *  laden) — Änderungen hier und dort gemeinsam.
 */

/** Ab wann ein ausbleibender Stand als Stillstand gilt.
 *
 *  Eine Minute ist das Fünfzehnfache des langsamsten Sicherheits-Polls (4 s
 *  bei gesundem Push-Kanal) und das Zweieinhalbfache der Frist, nach der ein
 *  toter Push-Kanal ohnehin auffliegt (25 s, `HERZSCHLAG_STILL_MS`). Wer hier
 *  anschlägt, hat ein echtes Problem — und keinen langsamen Tag. */
export const STILLSTAND_MS = 60_000;

/**
 * Steht die Anzeige still?
 *
 * @param {object} zustand
 * @param {number} zustand.startMs Wann die Seite geladen wurde.
 * @param {number} zustand.letzterAbrufOkMs Letzter fehlerfreie Abruf; 0 = noch keiner.
 * @param {number} zustand.letzterStandMs Letzter angewendete/bestätigte Stand; 0 = noch keiner.
 * @param {string|null} zustand.gemeldeteArt Was zuletzt gemeldet wurde (null = nichts).
 * @param {number} nowMs Jetzt.
 * @returns {{art: null|"verworfen"|"keine_abrufe", stillMs: number, melden: boolean, erholt: boolean}}
 */
export function lagePruefen(zustand, nowMs) {
  const z = zustand || {};
  const gemeldet = z.gemeldeteArt || null;
  const gesund = { art: null, stillMs: 0, melden: false, erholt: gemeldet !== null };

  const start = zahl(z.startMs);
  // Ohne Startzeitpunkt fehlt der Bezug — dann lieber gar nichts behaupten.
  if (!start) return { art: null, stillMs: 0, melden: false, erholt: false };

  // „Noch nie" zählt ab dem Laden der Seite: Eine Anzeige, die seit einer
  // Minute steht und nie etwas empfangen hat, ist genauso kaputt wie eine,
  // die es verloren hat.
  const abruf = zahl(z.letzterAbrufOkMs) || start;
  const stand = zahl(z.letzterStandMs) || start;

  const ohneAbruf = nowMs - abruf;
  const ohneStand = nowMs - stand;
  // Zeitstempel aus der Zukunft: Die Uhr ist gesprungen (Pis haben keine
  // Echtzeituhr und ziehen ihre Zeit per NTP nach). Kein Anlass für einen
  // Alarm — der nächste Durchgang rechnet wieder sauber.
  if (ohneAbruf < 0 || ohneStand < 0) return gesund;

  if (ohneAbruf >= STILLSTAND_MS) {
    // Die genauere Aussage gewinnt: Wenn schon gar nichts mehr ankommt,
    // ist „verworfen" nur eine Folge davon.
    return lage("keine_abrufe", ohneAbruf, gemeldet);
  }
  if (ohneStand >= STILLSTAND_MS) {
    return lage("verworfen", ohneStand, gemeldet);
  }
  return gesund;
}

function lage(art, stillMs, gemeldet) {
  return {
    art,
    stillMs,
    // Einmal je Episode. Solange dieselbe Lage anhält, schweigt der Wächter —
    // sonst liefe alle paar Sekunden ein Log-Upload los.
    melden: art !== gemeldet,
    erholt: false,
  };
}

function zahl(v) {
  return typeof v === "number" && isFinite(v) && v > 0 ? v : 0;
}
