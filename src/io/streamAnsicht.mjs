/** Logik der Stream-Einblendung (Spec `docs/features/monitor-stile-und-stream.md`,
 *  Etappe E2, ADR 0069): welche Felder, welche Optionen, wann sie verschwindet.
 *
 *  Aus der Adresse kommt nur, was hier durchgeht: Feld-IDs als positive
 *  Ganzzahlen (höchstens drei, ohne Doppelte), Optionen, die nur ABschalten
 *  können. Nation und Verein liefert der Server ohnehin nur, wenn die
 *  Turnierleitung sie eingeschaltet hat (ADR 0070).
 *
 *  Kanonische Fassung. `stream.html` trägt eine Inline-Kopie (die Assets
 *  durchlaufen keinen Build) — Änderungen hier und dort gemeinsam;
 *  `scripts/test-stream-ansicht.mjs` vergleicht beide wortgleich.
 */

export var MAX_FELDER = 3;
export var DATEN_FRIST_MS = 60000;

/** `?courts=1,2,3` → [1,2,3]; Unbrauchbares fällt weg, Doppelte auch. */
export function courtsAusQuery(roh) {
  var teile = typeof roh === "string" ? roh.split(",") : [];
  var ids = [];
  for (var i = 0; i < teile.length && ids.length < MAX_FELDER; i++) {
    var t = teile[i].trim();
    if (!/^[1-9][0-9]{0,9}$/.test(t)) continue;
    var id = Number(t);
    if (ids.indexOf(id) < 0) ids.push(id);
  }
  return ids;
}

/** Optionen aus der Adresse: nur `=0` schaltet ab, alles andere lässt an. */
export function optionenAusQuery(nation, verein, logo) {
  return { nation: nation !== "0", verein: verein !== "0", logo: logo !== "0" };
}

/** Zählt eine Antwort als Lebenszeichen des Turniers? Ein Relay ohne
 *  verbundenen Host antwortet mit leerem Feldnamen — das ist kein Stand. */
export function zaehltAlsDaten(state) {
  return !!state && typeof state.courtLabel === "string" && state.courtLabel !== "";
}

/** Sind die Daten jung genug, um sie zu zeigen? Ohne Daten nie. */
export function datenFrisch(letzterKontaktMs, jetztMs) {
  return letzterKontaktMs > 0 && jetztMs - letzterKontaktMs < DATEN_FRIST_MS;
}

/** Bekommt ein Feld eine Zeile? Nur mit Spiel. */
export function feldSichtbar(state) {
  return zaehltAlsDaten(state) && !!state.match;
}
