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
 * @param {unknown} [anordnung] Nur die Zähltafel: `nebeneinander` |
 *   `uebereinander` als Hand-Übersteuerung; `auto` oder Unbekanntes schreibt
 *   nichts in die Adresse (die Tafel folgt dann der Geräte-Ausrichtung).
 * @returns {string|null}
 */
export function zielPfad(ziel, spiegel, anordnung) {
  const court = ziel && Number.isInteger(ziel.court) && ziel.court > 0 ? ziel.court : null;
  switch (ziel && ziel.layout) {
    case "tafel": {
      if (!court) return null;
      const q = [];
      if (spiegel) q.push("spiegel=1");
      if (anordnung === "nebeneinander" || anordnung === "uebereinander") q.push("anordnung=" + anordnung);
      return `court/${court}/tafel${q.length ? "?" + q.join("&") : ""}`;
    }
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

// ── Ansicht der Zähltafel ────────────────────────────────────────────────
// Spiegelung und Anordnung sind für den Bediener EINE Frage: „Wo steht
// welches Team auf meinem Bildschirm?" Deshalb bündelt die Hülle beides zu
// einer Ansicht, die reihum geschaltet wird — per Tipp auf die Zahlen der
// Tafel (ohne PIN, das ist der Sinn) wie über den Menü-Eintrag. Das Etikett
// nennt zuerst, wo die **linke Tablet-Seite** steht, dann die rechte;
// `unten-oben` ist damit das ungespiegelte Übereinander (links = nah/unten).
// „auto + gespiegelt" gibt es bewusst nicht: Wer spiegelt, weiß, wie er
// sitzt — dann soll das Drehen des Geräts nichts mehr umwerfen.

export const ANSICHTEN = ["auto", "links-rechts", "rechts-links", "oben-unten", "unten-oben"];

export const ANSICHT_LABEL = {
  auto: "automatisch",
  "links-rechts": "links/rechts",
  "rechts-links": "rechts/links",
  "oben-unten": "oben/unten",
  "unten-oben": "unten/oben",
};

/** Ansicht → Parameter der Tafel-Adresse (`?spiegel=1`, `?anordnung=`). */
const ANSICHT_PARAMETER = {
  auto: { spiegel: false, anordnung: "auto" },
  "links-rechts": { spiegel: false, anordnung: "nebeneinander" },
  "rechts-links": { spiegel: true, anordnung: "nebeneinander" },
  "oben-unten": { spiegel: true, anordnung: "uebereinander" },
  "unten-oben": { spiegel: false, anordnung: "uebereinander" },
};

/** @param {unknown} roh Gemerkter Wert — alles Unbekannte gilt als `auto`. */
export function ansichtAusWert(roh) {
  return typeof roh === "string" && ANSICHTEN.includes(roh) ? roh : "auto";
}

/**
 * @param {unknown} ansicht
 * @returns {{spiegel:boolean, anordnung:"auto"|"nebeneinander"|"uebereinander"}}
 */
export function ansichtParameter(ansicht) {
  const p = ANSICHT_PARAMETER[ansichtAusWert(ansicht)];
  return { spiegel: p.spiegel, anordnung: p.anordnung };
}

/**
 * Umkehrung von {@link ansichtParameter} — für die einmalige Übernahme der
 * bis v0.9.288 getrennt gemerkten Schlüssel (Spiegel-Häkchen + Anordnung).
 * `auto + gespiegelt` war der Regelfall eines gespiegelten Tablets (das
 * Häkchen gesetzt, die Anordnung nie angefasst). Die Kombination entfällt,
 * der Spiegel bleibt aber: Er wird über die Ausrichtung des Geräts zur
 * festen Ansicht (Hochformat → `oben-unten`, sonst `rechts-links`).
 * @param {unknown} spiegel `true` oder `"1"` gilt als gespiegelt.
 * @param {unknown} anordnung Fehlend gilt als `auto`.
 * @param {boolean} [hochformat] Gerät steht hoch (Standard: nein).
 */
export function ansichtAusParametern(spiegel, anordnung, hochformat) {
  const sp = spiegel === true || spiegel === "1";
  const an = anordnung == null ? "auto" : anordnung;
  if (an === "auto") return sp ? (hochformat ? "oben-unten" : "rechts-links") : "auto";
  for (const a of ANSICHTEN) {
    const p = ANSICHT_PARAMETER[a];
    if (p.anordnung === an && p.spiegel === sp) return a;
  }
  return "auto";
}

/** Reihum; Unbekanntes zählt als `auto` und landet bei der ersten Hand-Ansicht. */
export function naechsteAnsicht(ansicht) {
  const i = ANSICHTEN.indexOf(ansichtAusWert(ansicht));
  return ANSICHTEN[(i + 1) % ANSICHTEN.length];
}
