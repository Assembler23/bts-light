/**
 * Zettel-Ereignisse am Tablet: Vereinigung und Undo-Schnitt.
 *
 * Kanonische Fassung (Spec `docs/features/schiedsrichterzettel-druck.md`,
 * ADR 0038). `src-tauri/assets/tablet.html` trägt eine **Inline-Kopie**
 * (die Assets durchlaufen keinen Build und können keine Module laden) —
 * Änderungen hier und dort gemeinsam.
 *
 * Warum die Logik überhaupt hier steht statt nur im Asset: Sie ist die
 * einzige Stelle, an der ein Bedienfehler unbemerkt Daten kosten kann.
 * Ein Undo, das eine Karte stehen lässt, fällt erst beim gedruckten
 * Zettel nach dem Turnier auf — deshalb ein Test-Harness mit `node`,
 * nicht nur Vertrauen.
 */

/** Höchstzahl Ereignisse je Match — gleich dem Deckel des Hosts. */
export const MAX_EVENTS = 64;

/**
 * Kanonische Ordnung `(set, afterN, seq, id)`.
 *
 * Die Kennung als letztes Kriterium ist kein Schmuck: Ohne sie hinge die
 * Reihenfolge zweier gleichzeitig erfasster Ereignisse davon ab, welches
 * zuerst eintraf — der gedruckte Zettel sähe je nach Netzlaune anders aus.
 */
export function sortiere(events) {
  return [...events].sort(
    (a, b) =>
      (a.set || 0) - (b.set || 0) ||
      (a.afterN || 0) - (b.afterN || 0) ||
      (a.seq || 0) - (b.seq || 0) ||
      String(a.id).localeCompare(String(b.id)),
  );
}

/**
 * Zwei Stände vereinigen: bekannte Kennungen ignorieren, unbekannte
 * aufnehmen, sortieren.
 *
 * **Idempotent und kommutativ** — genau wie im Host-Store (ADR 0038).
 * Deshalb ist es gleichgültig, in welcher Reihenfolge Tablet und Host
 * ihre Stände lernen, und ein Gerät, das offline war, bringt seinen
 * Rückstand einfach nach.
 *
 * Am Deckel gilt: Vorhandenes ist unantastbar. Passt nicht alles Neue,
 * werden die Neuen erst kanonisch geordnet und dann von vorn aufgenommen
 * — so hängt das Ergebnis nur an der Menge, nicht an der Reihenfolge.
 */
export function vereinigen(vorhanden, neu) {
  const bekannt = new Set((vorhanden || []).map((e) => e.id));
  const frisch = [];
  for (const e of neu || []) {
    if (!e || !e.id || bekannt.has(e.id)) continue;
    bekannt.add(e.id);
    frisch.push(e);
  }
  const platz = Math.max(0, MAX_EVENTS - (vorhanden || []).length);
  return sortiere([...(vorhanden || []), ...sortiere(frisch).slice(0, platz)]);
}

/**
 * Undo: Welche Ereignisse liegen jenseits des neuen Schnitts?
 *
 * Der Anker `(set, afterN)` ist eine **Position, keine Kennung**
 * (ADR 0038): Nach einem Undo wird dieselbe Nummer neu vergeben, ein
 * Ereignis „nach Ballwechsel 19" zeigte danach auf einen anderen
 * Ballwechsel. Deshalb werden die betroffenen Ereignisse ausdrücklich
 * zurückgenommen, statt sich stillschweigend zu verschieben.
 *
 * Liefert die Kennungen der Ereignisse, die im Satz `set` **nach**
 * `afterN` liegen und noch nicht zurückgenommen sind.
 */
export function undoSchnitt(events, set, afterN) {
  const zurueckgenommen = new Set(
    (events || []).filter((e) => e.kind === "retract").map((e) => e.retracts),
  );
  return (events || [])
    .filter(
      (e) =>
        e.kind !== "retract" &&
        !zurueckgenommen.has(e.id) &&
        (e.set || 0) === set &&
        (e.afterN || 0) > afterN,
    )
    .map((e) => e.id);
}

/**
 * Ist dieses Ereignis zurückgenommen?
 *
 * Gebraucht für die Anzeige: Ein zurückgenommenes Ereignis verschwindet
 * nicht, es wird durchgestrichen. Für einen Archivbeleg ist das ehrlicher
 * als spurloses Verschwinden.
 */
export function istZurueckgenommen(events, id) {
  return (events || []).some((e) => e.kind === "retract" && e.retracts === id);
}

/**
 * Der Anker eines Ereignisses, dessen Punkt gerade gebucht wurde.
 *
 * Klingt nach `afterN + 1` und ist es auch — aber die Stelle ist eine
 * Falle: Beendet der Punkt den Satz, hat das Tablet den Satz schon
 * weitergeschaltet, und ein *nachher* gelesener Anker zeigte auf den
 * nächsten, noch nicht begonnenen Satz mit `afterN = 0`. Die Karte
 * markierte dann einen Ballwechsel, den es nicht gibt, und der
 * Undo-Schnitt fand sie nie wieder, weil er im alten Satz sucht.
 * Deshalb: Anker VOR der Buchung nehmen und hier fortschreiben.
 */
export function ankerNachBuchung(ankerVorher) {
  return {
    set: ankerVorher.set,
    afterN: (ankerVorher.afterN || 0) + 1,
  };
}

/**
 * Eine Kennung: 12 Hex-Ziffern.
 *
 * Der Host verlangt eine reine Hex-Folge (sie wird verglichen, sortiert
 * und protokolliert) — alles andere weist er ab. `crypto.getRandomValues`
 * statt `Math.random`, damit zwei Tablets, die im selben Moment starten,
 * nicht dieselbe Kennung erzeugen.
 */
export function neueKennung(zufall) {
  const bytes = new Uint8Array(6);
  if (zufall) {
    zufall(bytes);
  } else if (typeof crypto !== "undefined" && crypto.getRandomValues) {
    crypto.getRandomValues(bytes);
  } else {
    for (let i = 0; i < bytes.length; i += 1) {
      bytes[i] = Math.floor(Math.random() * 256);
    }
  }
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}
