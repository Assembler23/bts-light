/** Wann gilt der Push-Kanal einer Anzeige als gesund?
 *  (Spec `docs/features/monitor-livestand-push.md`, Etappe S6)
 *
 *  **Das höchste Risiko im ganzen Vorhaben.** Eine falsche Regel hier friert
 *  alle Anzeigen einer Halle ein: Hält die Seite einen toten Kanal für
 *  gesund, verlangsamt sie ihren Sicherheits-Poll auf vier Sekunden — und
 *  wenn dann auch der nichts mehr bringt, steht der Spielstand still,
 *  während auf dem Feld weitergezählt wird.
 *
 *  Die alte Regel („Socket offen **und** letzter Anstoß < 1,2 s her")
 *  verwechselte **„der Kanal lebt"** mit **„es passiert gerade etwas"**.
 *  Solange der Fallback viermal je Sekunde lief, fiel das nicht auf. Mit dem
 *  langsameren Takt schon: Eine ruhige Halle zwischen zwei Ballwechseln sähe
 *  aus wie ein toter Kanal — und ein halbtoter Socket meldet umgekehrt
 *  minutenlang `OPEN`, ohne dass je etwas ankommt.
 *
 *  Deshalb zählt jetzt **jedes** Frame vom Server, auch der Herzschlag alle
 *  zehn Sekunden. Und drei Dinge müssen zusammen stimmen:
 *
 *  1. Der Socket ist offen.
 *  2. Das letzte Frame (Anstoß **oder** Herzschlag) ist keine 25 Sekunden her.
 *  3. Der letzte Abruf hat geklappt — **ein einziger** Fehlversuch genügt,
 *     um sofort auf den schnellen Takt zurückzufallen. Lieber ein paar
 *     überflüssige Abrufe als eine Anzeige, die einen Ausfall verschläft.
 *
 *  Kanonische Fassung. `overview.html` und `monitor.html` tragen eine
 *  Inline-Kopie (die Assets durchlaufen keinen Build) — Änderungen hier und
 *  dort gemeinsam.
 */

/** Ab wann ein ausbleibender Herzschlag den Kanal für tot erklärt.
 *  Zweieinhalb Herzschläge: Ein einzelner verlorener soll noch nichts
 *  auslösen. Gleicher Wert wie `MONITOR_HEARTBEAT_STALE_MS` im Rust-Teil. */
export const HERZSCHLAG_STILL_MS = 25000;

/** Sicherheits-Poll bei gesundem Kanal (nur mit gesetztem Schalter). */
export const FALLBACK_LANGSAM_MS = 4000;

/** Sicherheits-Poll sonst — und immer beim Start, bis der Kanal sich als
 *  gesund erwiesen hat. */
export const FALLBACK_SCHNELL_MS = 250;

/**
 * Ist der Push-Kanal gesund?
 *
 * @param {object} zustand
 * @param {boolean} zustand.wsOpen Socket offen?
 * @param {number} zustand.lastServerFrameMs Zeitpunkt des letzten Frames (Anstoß ODER Herzschlag); 0 = noch keins.
 * @param {boolean} zustand.lastFetchOk Hat der letzte Abruf geklappt?
 * @param {number} zustand.failures Zahl der Fehlversuche in Folge.
 * @param {number} nowMs Jetzt.
 * @returns {boolean}
 */
export function pushGesund(zustand, nowMs) {
  const z = zustand || {};
  if (!z.wsOpen) return false;
  if (z.lastFetchOk === false) return false;
  if ((z.failures || 0) > 0) return false;
  const letztes = z.lastServerFrameMs || 0;
  // Noch nie etwas empfangen: Der Kanal hat sich nicht bewährt — bis dahin
  // gilt der schnelle Takt.
  if (letztes <= 0) return false;
  return nowMs - letztes < HERZSCHLAG_STILL_MS;
}

/**
 * Wie schnell soll der Sicherheits-Poll laufen?
 *
 * Ohne den Schalter bleibt es beim schnellen Takt — auch bei gesundem
 * Kanal. Eine frisch aktualisierte Installation verhält sich damit exakt wie
 * vorher.
 *
 * @param {boolean} gesund Ergebnis von {@link pushGesund}.
 * @param {boolean} langsamErlaubt Schalter `pushFallbackSlow` aus der Konfiguration.
 * @returns {number} Millisekunden.
 */
export function fallbackTakt(gesund, langsamErlaubt) {
  return gesund && langsamErlaubt ? FALLBACK_LANGSAM_MS : FALLBACK_SCHNELL_MS;
}

/**
 * Ist der Kanal so lange still, dass die Verbindung aktiv erneuert werden
 * sollte?
 *
 * Bewusste Umkehr des früheren Verhaltens („kein Force-Close bei Stille").
 * Das galt nur, weil der 250-ms-Poll ohnehin alles auffing. Ein Socket, der
 * `OPEN` meldet, aber nichts mehr liefert, ist heute nur noch am
 * ausbleibenden Herzschlag zu erkennen.
 *
 * @param {boolean} wsOpen
 * @param {number} lastServerFrameMs
 * @param {number} nowMs
 * @returns {boolean}
 */
export function kanalIstTot(wsOpen, lastServerFrameMs, nowMs) {
  if (!wsOpen) return false; // nicht offen → der Reconnect läuft ohnehin
  const letztes = lastServerFrameMs || 0;
  if (letztes <= 0) return false; // noch nie etwas empfangen: erst abwarten
  return nowMs - letztes >= HERZSCHLAG_STILL_MS;
}
