// Schlankes Undo-Format des Zähl-Tablets (Spec tablet-stand-robust, ADR 0084).
//
// Der Tablet-Zustand reist je Punkt an den Turnier-PC bzw. das Relay. 96 %
// davon waren die 50 Undo-Stände, weil jeder eine Vollkopie des Punktverlaufs
// (und der Zettel-Ereignisse) trug. Ab Format 2 trägt ein Undo-Stand statt
// der Vollkopie nur die LÄNGEN des Verlaufs je Satz (`rl`); Wiederherstellen
// kürzt den aktuellen Verlauf auf diese Längen.
//
// Das trägt, weil Undo strikt von oben nach unten läuft und der Verlauf
// zwischen zwei Undo-Ständen nur WÄCHST. Die einzige Operation, die ihn
// umschreibt, ist „Mannschaften tauschen" (ADR 0067) — ihr Undo-Stand behält
// deshalb die Vollkopie (`rallyLog`). Jede künftige Operation, die den
// Verlauf umschreibt, muss das ebenso tun.
//
// `src-tauri/assets/tablet.html` trägt eine Inline-Kopie (kein Modul-Import
// möglich); `scripts/test-inline-kopien.mjs` vergleicht ihr Verhalten mit
// diesem Modul. Deshalb: schlichte `function`-Deklarationen, kein Export
// von Pfeilfunktionen.

/** Ab dieser Länge (**Bytes**, UTF-8) kürzt das Tablet den Spiegel selbst.
 *  Der Deckel bei Host und Relay ist 64 KiB Bytes; gemessen wird deshalb
 *  auch hier in Bytes (`utf8Laenge`), nicht in Zeichen — sonst läge ein
 *  Zustand mit vielen Mehrbyte-Zeichen in den Namen unter dem Budget und
 *  trotzdem über dem Deckel. Die Reserve gilt dem Frame-Rahmen. */
export const SPIEGEL_BUDGET = 56000;

/** Länge von `s` in UTF-8-Bytes, ohne den Text zu kodieren. Ein
 *  Ersatzzeichen-Paar zählt 4 (je Hälfte 2). */
export function utf8Laenge(s) {
  var n = 0;
  for (var i = 0; i < s.length; i++) {
    var c = s.charCodeAt(i);
    n += c < 0x80 ? 1 : c < 0x800 ? 2 : c >= 0xd800 && c <= 0xdfff ? 2 : 3;
  }
  return n;
}

/** Hat `log` die Form eines Punktverlaufs? Der Zustand kann von einem
 *  fremden Gerät stammen (Übernahme über Server/Relay) oder aus einem
 *  beschädigten `localStorage` — ein Wurf beim Laden hinterließe einen halb
 *  überschriebenen Spielstand. */
export function verlaufGueltig(log) {
  if (!log || typeof log !== "object" || !Array.isArray(log.sets)) return false;
  for (var i = 0; i < log.sets.length; i++) {
    var s = log.sets[i];
    if (!s || typeof s !== "object" || typeof s.points !== "string") return false;
  }
  return true;
}

/** Längen-Schnitt eines Verlaufs: `[[Punkte je Satz …], midGame]`, oder
 *  `null`, wenn nie etwas aufgezeichnet wurde (oder er unlesbar ist). */
export function verlaufSchnitt(log) {
  if (!verlaufGueltig(log)) return null;
  return [log.sets.map(function (s) { return s.points.length; }), !!log.midGame];
}

/** Kürzt `log` auf den Schnitt `rl`. Passt der Schnitt nicht zum Verlauf
 *  (er ist kürzer als verlangt), gibt es `null` — der nächste Ballwechsel
 *  beginnt dann ehrlich „ab Zwischenstand", statt einen falschen Verlauf
 *  stehen zu lassen. */
export function verlaufAufSchnitt(log, rl) {
  if (!Array.isArray(rl) || !Array.isArray(rl[0]) || !verlaufGueltig(log)) return null;
  var laengen = rl[0];
  if (log.sets.length < laengen.length) return null;
  var sets = [];
  for (var i = 0; i < laengen.length; i++) {
    var s = log.sets[i];
    var len = laengen[i];
    if (typeof len !== "number" || !(len >= 0) || len % 1 !== 0 || s.points.length < len) return null;
    sets.push({ startA: s.startA, startB: s.startB, points: s.points.slice(0, laengen[i]) });
  }
  return Object.assign({}, log, { sets: sets, midGame: !!rl[1] });
}

/** Der Verlauf, den ein Undo-Stand wiederherstellt. Vollkopie (altes Format
 *  oder Korrektur-Schritt) hat Vorrang; sonst der Schnitt; sonst `null`. */
export function verlaufAusStand(snap, aktuell) {
  if (verlaufGueltig(snap.rallyLog)) return JSON.parse(JSON.stringify(snap.rallyLog));
  if (snap.rl) return verlaufAufSchnitt(aktuell, snap.rl);
  return null;
}

/** Ist `alt` genau das, was beim Kürzen von `bezug` auf die Längen von
 *  `alt` herauskommt? Nur dann darf eine Vollkopie zum Schnitt werden. */
export function verlaufIstPraefix(alt, bezug) {
  var gekuerzt = verlaufAufSchnitt(bezug, verlaufSchnitt(alt));
  return !!gekuerzt && JSON.stringify(gekuerzt) === JSON.stringify(alt);
}

/** Bringt eine geladene Undo-Liste (beliebiges Format) ins Format 2:
 *  Ereignis-Kopien fallen weg (niemand liest sie, ADR 0038), Vollkopien des
 *  Verlaufs werden zum Schnitt — aber nur, wo sie nachweislich ein Präfix
 *  des Bezugs sind. Der Bezug ist der Verlauf, der beim Wiederherstellen
 *  des jeweils jüngeren Stands gilt; er wandert von oben nach unten mit.
 *  Gibt eine neue Liste zurück, die Eingabe bleibt unberührt. */
export function historyNormalisieren(history, rallyLog) {
  var aus = [];
  if (!Array.isArray(history)) return aus;
  var bezug = verlaufGueltig(rallyLog) ? rallyLog : null;
  for (var i = history.length - 1; i >= 0; i--) {
    var roh = history[i];
    // Kein Undo-Stand (fremder/beschädigter Zustand) → weglassen statt
    // später beim „zurück" daran zu scheitern.
    if (!roh || typeof roh !== "object" || Array.isArray(roh)) continue;
    var s = {};
    for (var k in roh) {
      // `__proto__` aus JSON ist ein gewöhnlicher Schlüssel — beim Kopieren
      // würde er zum Prototyp der Kopie.
      if (Object.prototype.hasOwnProperty.call(roh, k) && k !== "__proto__" && k !== "events") s[k] = roh[k];
    }
    if (verlaufGueltig(s.rallyLog)) {
      var kippt = !!(s.korrektur && s.korrektur.art === "ab");
      if (!kippt && verlaufIstPraefix(s.rallyLog, bezug)) {
        bezug = s.rallyLog;
        s.rl = verlaufSchnitt(s.rallyLog);
        delete s.rallyLog;
      } else {
        bezug = s.rallyLog;
        delete s.rl;
      }
    } else {
      // Schnitt (Format 2) oder gar kein Verlauf (null / uralter Stand).
      bezug = s.rl ? verlaufAufSchnitt(bezug, s.rl) : null;
      delete s.rallyLog;
      if (s.rl === undefined) s.rl = null;
    }
    aus.unshift(s);
  }
  return aus;
}

/** Der JSON-Text für den Spiegel. Passt `json` ins Budget, bleibt er
 *  unverändert. Sonst fallen die ÄLTESTEN Undo-Stände weg, bis er passt —
 *  nur im Spiegel; der lokale Stand behält alles. So verwirft niemand den
 *  Zustand still (Spec tablet-stand-robust, Regel B). */
export function spiegelKuerzen(snap, json, budget) {
  var gesamt = utf8Laenge(json);
  if (gesamt <= budget) return json;
  var h = Array.isArray(snap.history) ? snap.history : [];
  var ueber = gesamt - budget;
  var weg = 0;
  while (weg < h.length && ueber > 0) {
    ueber -= utf8Laenge(JSON.stringify(h[weg])) + 1;
    weg++;
  }
  return JSON.stringify(Object.assign({}, snap, { history: h.slice(weg) }));
}
