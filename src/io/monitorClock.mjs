/** Server-Uhr-Abgleich für die Monitor-Anzeigen (Etappe ETag, 1c).
 *
 *  Seit dem ETag/304-Umbau bekommt die Anzeige nur noch bei einem vollen
 *  200-Abruf ein frisches `serverNowMs`; dazwischen antwortet der Server mit
 *  304 (kein Body). Damit die Pausen-/Aufruf-Uhren nicht im 250-ms-Takt
 *  stehen bleiben, hält dieses Modul einen laufenden **Offset** zur lokalen
 *  Uhr: `nowServerMs() = Date.now() + offset`.
 *
 *  Gespeist wird der Offset aus ZWEI Quellen:
 *   - jedem 200-Abruf (`state.serverNowMs`),
 *   - dem WS-Herzschlag (`{"hb":<nowMs>}`), den der Server auch dann sendet,
 *     wenn sich ansonsten nichts tut.
 *
 *  Damit überlebt die Uhr eine 304-Serie und einen WS-Abriss (U12): Der
 *  Sicherheits-Poll erzwischt über den Refetch-Cap regelmäßig einen 200, und
 *  der Herzschlag hält den Offset zwischen vollen Abrufen frisch.
 *
 *  Kanonische Fassung. `monitor.html` trägt eine Inline-Kopie (die Assets
 *  durchlaufen keinen Build) — Änderungen hier und dort gemeinsam.
 */

/** Aktueller Offset `serverNowMs - Date.now()`; `null` = noch nie gefüttert. */
let offset = null;

/**
 * Füttert den Offset mit einer frischen Server-Zeit.
 *
 * @param {number} serverNowMs Server-Zeit (ms seit Epoch); 0/falsch wird
 *   ignoriert (alte Frames ohne das Feld).
 * @param {number} [jetztMs] Lokale Uhr beim Empfang (für Tests injizierbar).
 * @returns {boolean} true, wenn der Offset neu gesetzt wurde.
 */
export function clockFeed(serverNowMs, jetztMs) {
  if (typeof serverNowMs !== "number" || serverNowMs <= 0) return false;
  offset = serverNowMs - (jetztMs || Date.now());
  return true;
}

/**
 * Server-Zeit „jetzt". Ohne bekannten Offset (Kaltstart) die lokale Uhr.
 *
 * @param {number} [jetztMs] Lokale Uhr (für Tests injizierbar).
 * @returns {number} ms seit Epoch, relativ zur Server-Uhr.
 */
export function nowServerMs(jetztMs) {
  const j = jetztMs || Date.now();
  return typeof offset === "number" ? j + offset : j;
}

/** Hat ein Offset bereits gesetzt? (für Tests und Logik-Abfragen) */
export function clockHatOffset() {
  return typeof offset === "number";
}
