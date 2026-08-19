/**
 * Zettel-Ereignisse am Tablet (Spec schiedsrichterzettel-druck, ADR 0038).
 *
 * Prüft die kanonische Fassung aus `src/io/matchEvents.mjs`. Die
 * Inline-Kopie in `src-tauri/assets/tablet.html` muss gemeinsam mit ihr
 * geändert werden — `scripts/check-asset-syntax.mjs` prüft dort nur die
 * Syntax, nicht die Deckungsgleichheit.
 */
import assert from "node:assert/strict";
import {
  MAX_EVENTS,
  sortiere,
  vereinigen,
  undoSchnitt,
  istZurueckgenommen,
  ankerNachBuchung,
  neueKennung,
} from "../src/io/matchEvents.mjs";

const ev = (id, set, afterN, seq, kind = "card_yellow") => ({
  id,
  set,
  afterN,
  seq,
  kind,
});

// ── Ordnung ───────────────────────────────────────────────────────────
{
  const gemischt = [
    ev("d4", 2, 3, 1),
    ev("a1", 1, 0, 1),
    ev("c3", 1, 9, 2),
    ev("b2", 1, 9, 1),
  ];
  assert.deepEqual(
    sortiere(gemischt).map((e) => e.id),
    ["a1", "b2", "c3", "d4"],
  );
  // Gleicher Anker und gleiche seq: die Kennung entscheidet — sonst hinge
  // das gedruckte Bild an der Netzlaune.
  assert.deepEqual(
    sortiere([ev("ff", 1, 0, 1), ev("00", 1, 0, 1)]).map((e) => e.id),
    ["00", "ff"],
  );
}

// ── Vereinigung: idempotent und kommutativ ────────────────────────────
{
  const a = ev("a1", 1, 0, 1);
  const b = ev("b2", 1, 5, 2);
  const c = ev("c3", 2, 1, 3);

  const eins = vereinigen(vereinigen(vereinigen([], [a, b]), [b, c]), [a, b, c]);
  const zwei = vereinigen(vereinigen([], [c, b]), [a]);
  assert.deepEqual(
    eins.map((e) => e.id),
    zwei.map((e) => e.id),
    "Reihenfolge darf das Ergebnis nicht ändern",
  );
  assert.deepEqual(
    eins.map((e) => e.id),
    ["a1", "b2", "c3"],
  );

  // Ein Ersatz-Tablet mit leerem Stand darf nichts wegnehmen.
  assert.deepEqual(
    vereinigen([a, b, c], []).map((e) => e.id),
    ["a1", "b2", "c3"],
  );
  // Dasselbe Ereignis zweimal ändert nichts.
  assert.equal(vereinigen([a], [a]).length, 1);
}

// ── Deckel: Vorhandenes ist unantastbar ───────────────────────────────
{
  const voll = Array.from({ length: MAX_EVENTS }, (_, i) =>
    ev(String(i).padStart(4, "0"), 1, 0, i),
  );
  const neu = ev("ffff", 1, 0, 999);
  const ergebnis = vereinigen(voll, [neu]);
  assert.equal(ergebnis.length, MAX_EVENTS, "Deckel hält");
  assert.ok(
    !ergebnis.some((e) => e.id === "ffff"),
    "am Deckel kommt nichts Neues mehr hinein",
  );
  assert.equal(
    voll.filter((e) => ergebnis.some((r) => r.id === e.id)).length,
    MAX_EVENTS,
    "und Vorhandenes bleibt vollständig",
  );

  // Am Deckel darf die Auswahl nicht an der Reihenfolge hängen.
  const fast = voll.slice(0, MAX_EVENTS - 2);
  const drei = [ev("aaaa", 2, 1, 1), ev("bbbb", 2, 2, 2), ev("cccc", 2, 3, 3)];
  assert.deepEqual(
    vereinigen(fast, drei).map((e) => e.id),
    vereinigen(fast, [...drei].reverse()).map((e) => e.id),
  );
}

// ── Undo-Schnitt ──────────────────────────────────────────────────────
{
  const events = [
    ev("a1", 1, 3, 1),
    ev("b2", 1, 7, 2),
    ev("c3", 1, 9, 3),
    ev("d4", 2, 1, 4),
  ];
  // Schnitt bei Ballwechsel 5 in Satz 1: b2 und c3 liegen dahinter.
  assert.deepEqual(undoSchnitt(events, 1, 5), ["b2", "c3"]);
  // Der andere Satz bleibt unberührt.
  assert.deepEqual(undoSchnitt(events, 2, 0), ["d4"]);
  // Nichts dahinter: nichts zurückzunehmen.
  assert.deepEqual(undoSchnitt(events, 1, 99), []);

  // Bereits Zurückgenommenes wird nicht doppelt zurückgenommen.
  const mitRuecknahme = [
    ...events,
    { id: "e5", set: 1, afterN: 9, seq: 5, kind: "retract", retracts: "c3" },
  ];
  assert.deepEqual(undoSchnitt(mitRuecknahme, 1, 5), ["b2"]);
  assert.ok(istZurueckgenommen(mitRuecknahme, "c3"));
  assert.ok(!istZurueckgenommen(mitRuecknahme, "b2"));
}

// ── Anker über eine Satzgrenze (Review-Befund E6) ─────────────────────
{
  // Der gefährliche Fall: Die rote Karte gewinnt den SATZ. Das Tablet
  // schaltet dann sofort weiter — ein nachher gelesener Anker zeigte auf
  // den nächsten, noch nicht begonnenen Satz.
  const vorPunkt = { set: 1, afterN: 38 };
  const richtig = ankerNachBuchung(vorPunkt);
  assert.deepEqual(richtig, { set: 1, afterN: 39 });

  // So sah es aus, als der Anker NACH der Buchung gelesen wurde.
  const falsch = { set: 2, afterN: 0 };

  const karte = { id: "cc", ...richtig, seq: 1, kind: "card_red" };
  const karteFalsch = { id: "cc", ...falsch, seq: 1, kind: "card_red" };

  // Undo dreht auf den Stand vor dem Punkt zurück und schneidet dort.
  // Mit richtigem Anker findet der Schnitt die Karte …
  assert.deepEqual(undoSchnitt([karte], vorPunkt.set, vorPunkt.afterN), ["cc"]);
  // … mit dem falschen NICHT: Sie liegt in einem anderen Satz und
  // überlebte das Undo für immer.
  assert.deepEqual(undoSchnitt([karteFalsch], vorPunkt.set, vorPunkt.afterN), []);

  // Auch mitten im Satz muss die Regel gelten.
  assert.deepEqual(ankerNachBuchung({ set: 2, afterN: 0 }), { set: 2, afterN: 1 });
  assert.deepEqual(ankerNachBuchung({ set: 3 }), { set: 3, afterN: 1 });
}

// ── Kennungen ─────────────────────────────────────────────────────────
{
  // Der Host verlangt reine Hex-Folgen — alles andere weist er ab.
  for (let i = 0; i < 200; i += 1) {
    const id = neueKennung();
    assert.match(id, /^[0-9a-f]{12}$/, `keine 12 Hex-Ziffern: ${id}`);
  }
  // Deterministisch prüfbar: Führende Nullen dürfen nicht wegfallen.
  assert.equal(
    neueKennung((bytes) => bytes.fill(0)),
    "000000000000",
  );
  assert.equal(
    neueKennung((bytes) => bytes.fill(255)),
    "ffffffffffff",
  );
  // Zwei Aufrufe kollidieren praktisch nie — schützt vor einem
  // versehentlich konstanten Generator.
  assert.notEqual(neueKennung(), neueKennung());
}

console.log("test-match-events: alle Fälle grün");
