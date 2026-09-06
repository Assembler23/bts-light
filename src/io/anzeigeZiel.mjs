/** Ziel-Allowlist und Pfadbau der Anzeige-Hülle (Spec
 *  `docs/features/zaehltafel-anzeige-huelle.md`, Abschnitt „Anzeige-Hülle").
 *
 *  Die Hülle bettet eine Anzeige-Seite in ein iframe. Was dort geladen wird,
 *  entscheidet ausschließlich dieses Modul: vier feste Layouts, eine CourtID
 *  als positive Ganzzahl — nie freier Text aus der Adresse. Ein unbekanntes
 *  Layout fällt auf die Zähltafel zurück, eine unbrauchbare CourtID wird
 *  `null` (die Hülle öffnet dann die Feldwahl).
 *
 *  Pfade sind relativ ohne führenden Schrägstrich, weil `BASE` (LAN `/`,
 *  Cloud `/bts-relay/<ns>/`) davor kommt.
 *
 *  Kanonische Fassung. `anzeige.html` trägt eine Inline-Kopie (die Assets
 *  durchlaufen keinen Build) — Änderungen hier und dort gemeinsam.
 */

export const LAYOUTS = ["tafel", "feld", "uebersicht", "vorbereitung"];

/** Braucht das Layout ein Feld? */
export function feldbezogen(layout) {
  return layout === "tafel" || layout === "feld";
}

/**
 * @param {unknown} layoutRoh Wert aus `?layout=` (oder gemerkt).
 * @param {unknown} courtRoh Wert aus `?court=` (oder gemerkt).
 * @returns {{layout:string, court:number|null}}
 */
export function zielAusQuery(layoutRoh, courtRoh) {
  const l = typeof layoutRoh === "string" ? layoutRoh : "";
  const layout = LAYOUTS.includes(l) ? l : "tafel";
  const c = courtRoh == null ? "" : String(courtRoh);
  // Nur 1–10 Ziffern ohne führende Null: keine Vorzeichen, Brüche,
  // Exponenten, Pfadteile oder Leerzeichen.
  const court = /^[1-9][0-9]{0,9}$/.test(c) ? Number(c) : null;
  return { layout, court };
}

/**
 * @param {{layout:string, court:number|null}} ziel
 * @param {boolean} spiegel Nur die Zähltafel kennt `?spiegel=1`.
 * @returns {string|null}
 */
export function zielPfad(ziel, spiegel) {
  const court = ziel && Number.isInteger(ziel.court) && ziel.court > 0 ? ziel.court : null;
  switch (ziel && ziel.layout) {
    case "tafel":
      return court ? `court/${court}/tafel${spiegel ? "?spiegel=1" : ""}` : null;
    case "feld":
      return court ? `court/${court}/display` : null;
    case "uebersicht":
      return "info/overview";
    case "vorbereitung":
      return "info/preparation";
    default:
      return null;
  }
}
