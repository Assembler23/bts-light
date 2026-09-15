/** Fünf-Minuten-Freigabe der Zahnrad-PIN (Zählseite und Anzeige-Hülle).
 *
 *  Wer die Einstellungs-PIN einmal richtig eingegeben hat, soll das Zahnrad
 *  fünf Minuten lang ohne erneute Eingabe öffnen können (Wunsch 08.09.2026):
 *  Feld wechseln, Anordnung umstellen, neu laden — das sind oft mehrere
 *  Tipps hintereinander, und jeder Feldwechsel lädt die Seite neu. Darum
 *  lebt der Ablauf-Zeitpunkt (Epoch-Millisekunden) im `localStorage` des
 *  Geräts und nicht nur im Seitenzustand.
 *
 *  Die Frist läuft ab der Eingabe und wird durch weitere Bedienung NICHT
 *  verlängert. Ein Wert, der weiter als die Frist in der Zukunft liegt, ist
 *  keine echte Freigabe (zurückgestellte Uhr, manipulierter Speicher) und
 *  gilt als abgelaufen. Die PIN bleibt Bedienschutz, keine Sicherheitsgrenze
 *  — siehe `docs/tablet-kiosk.md`.
 *
 *  Kanonische Fassung. `tablet.html` und `anzeige.html` tragen Inline-Kopien
 *  (die Assets durchlaufen keinen Build) — Änderungen hier und dort
 *  gemeinsam.
 */

export const FREIGABE_MS = 5 * 60 * 1000;

/** Ablauf-Zeitpunkt einer frisch eingegebenen PIN. */
export function neueFreigabe(jetztMs) {
  return jetztMs + FREIGABE_MS;
}

/** Gilt die Freigabe zum Zeitpunkt `jetztMs` noch? */
export function freigabeGueltig(bisMs, jetztMs) {
  if (typeof bisMs !== "number" || !Number.isFinite(bisMs) || bisMs <= 0) return false;
  if (bisMs <= jetztMs) return false;
  return bisMs - jetztMs <= FREIGABE_MS;
}

/** Gespeicherten Wert (String aus `localStorage`) zurück in Millisekunden — oder `null`. */
export function freigabeLesen(roh) {
  if (typeof roh !== "string" || !/^\d{1,16}$/.test(roh)) return null;
  const n = Number(roh);
  return n > 0 ? n : null;
}
