/** Monitoransicht „Begegnung“ für Punktspiele (Spec
 *  `docs/features/punktspiel-monitoransicht.md`).
 *
 *  Reine Logik der Seite `assets/begegnung.html`: Initialen für die Kachel
 *  ohne Logo, Texte je Zustand, Zeilenzahl und Schriftgröße. Gezählt und
 *  gemischt wird NICHT hier, sondern im Kern (`relay_proto::begegnung_anzeige`)
 *  — die Seite zeigt nur, was die Stand-Route liefert.
 *
 *  Kanonische Fassung. `begegnung.html` trägt zwischen den Marken
 *  `>>> begegnungAnsicht` und `<<< begegnungAnsicht` eine wortgleiche Kopie
 *  ohne `export` (die Assets durchlaufen keinen Build). Deshalb bewusst in
 *  ES5 geschrieben. `scripts/test-begegnung-ansicht.mjs` prüft die Gleichheit.
 */

// >>> begegnungAnsicht
/** So viele Spiele zeigt die Seite höchstens — ohne Blättern, ohne Laufband. */
export var MAX_ZEILEN = 12;
/** Text für einen Platz, der in der Aufstellung noch fehlt. */
export var PLATZ_OFFEN = "noch nicht aufgestellt";

/** Initialen für die Kachel ohne Logo: die Anfangsbuchstaben der ersten zwei
 *  Wörter, die mit einem Buchstaben beginnen. „1. BC Muster 2“ → „BM“. */
export function initialen(name) {
  var woerter = String(name == null ? "" : name).split(/[\s.\-\/]+/);
  var aus = "";
  for (var i = 0; i < woerter.length && aus.length < 2; i++) {
    var erster = woerter[i].charAt(0);
    // Buchstabe = hat Groß- und Kleinform (Ziffern und Zeichen nicht).
    if (erster && erster.toLowerCase() !== erster.toUpperCase()) aus += erster.toUpperCase();
  }
  return aus || "?";
}

/** Namen einer Seite, aufgefüllt auf die erwartete Zahl (Einzel 1, Doppel 2).
 *  Ein fehlender Platz heißt [`PLATZ_OFFEN`]; ein schon aufgestellter
 *  Doppelpartner bleibt sichtbar. Liefert `[{ text, offen }]`. */
export function namenListe(namen, doppel) {
  var liste = Array.isArray(namen) ? namen : [];
  var soll = doppel ? 2 : 1;
  var aus = [];
  for (var i = 0; i < soll; i++) {
    var n = typeof liste[i] === "string" ? liste[i].trim() : "";
    aus.push(n ? { text: n, offen: false } : { text: PLATZ_OFFEN, offen: true });
  }
  return aus;
}

/** Text der Zustandsspalte. Leer heißt: nichts zu sagen. */
export function zustandText(zeile) {
  var z = zeile || {};
  if (z.zustand === "laeuft") return z.feld != null ? "Feld " + z.feld : "läuft";
  if (z.zustand === "wird_uebermittelt") return "Ergebnis wird übermittelt";
  if (z.zustand === "beendet") {
    if (z.ausgang === "kampflos") return "kampflos";
    if (z.ausgang === "aufgabe") return "Aufgabe";
    if (z.ausgang === "disqualifikation") return "Disqualifikation";
  }
  return "";
}

/** Nebenzeile unter dem Gesamtstand: Sätze und Punkte. */
export function nebenzeile(stand) {
  var h = (stand && stand.heim) || {}, g = (stand && stand.gast) || {};
  function zahl(v) { return typeof v === "number" && isFinite(v) ? v : 0; }
  return zahl(h.saetze) + " : " + zahl(g.saetze) + " Sätze · " +
    zahl(h.punkte) + " : " + zahl(g.punkte) + " Punkte";
}

/** Die Zeilen, die auf den Bildschirm passen. */
export function sichtbareZeilen(spiele) {
  return (Array.isArray(spiele) ? spiele : []).slice(0, MAX_ZEILEN);
}

/** Höhe einer Zeile und Schriftgrößen in Pixeln: Die Zeilen teilen sich die
 *  verfügbare Höhe, die Schrift folgt der Zeilenhöhe. Mit Doppeln stehen zwei
 *  Namen übereinander, die Namen sind dann kleiner. Gerechnet wird immer mit
 *  mindestens vier Zeilen, damit zwei Spiele nicht bildschirmhohe Buchstaben
 *  bekommen. */
export function zeilenMass(hoehe, anzahl, mitDoppel) {
  var n = Math.max(4, Math.min(MAX_ZEILEN, Math.floor(anzahl) || 0));
  var h = Math.max(0, Number(hoehe) || 0);
  var abstand = h * 0.012;
  var zeile = Math.max(0, (h - abstand * (n - 1)) / n);
  return {
    zeile: zeile,
    abstand: abstand,
    name: zeile * (mitDoppel ? 0.38 : 0.46),
    satz: zeile * 0.56,
  };
}
// <<< begegnungAnsicht
