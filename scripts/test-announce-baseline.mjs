// Baseline-Logik der Feld-Ansage (src/io/announceBaseline.mjs).
//
// Der Bug, den dieser Test festhält (gemeldet 14.08.2026): Beim Start sagte
// BTS Light ALLE Spiele an, die gerade auf den Feldern stehen. Grund war
// nicht das Fehlen eines Baseline-Schutzes — den gab es —, sondern dass er
// auf einem LEEREN Stand gesetzt wurde: Der erste Abruf kommt, bevor der
// Sync-Lauf seinen ersten BTP-Schnappschuss hat, und liefert null Felder.
// Damit galt beim zweiten Abruf jedes belegte Feld als frisch aufgerufen.
//
// Ausfuehren: node scripts/test-announce-baseline.mjs

import { diffOccupiedCourts } from "../src/io/announceBaseline.mjs";

let fehler = 0;
function pruefe(label, bedingung) {
  console.log((bedingung ? "  ok   " : "  FAIL ") + label);
  if (!bedingung) fehler++;
}

/** Ein belegtes Feld. */
const feld = (courtId, matchId, location = "") => ({
  court_id: courtId,
  match_id: matchId,
  location,
});

console.log("=== Start mitten im Turnier: nichts ansagen ===");
{
  // Erster Abruf hat schon Daten (App-Neustart bei laufenden Spielen).
  let stand = { baseline: new Map(), hatBaseline: false };
  const eins = diffOccupiedCourts(stand, [feld(1, 100), feld(2, 200)], "");
  pruefe("erster Abruf sagt nichts an", eins.neue.length === 0);
  pruefe("Baseline ist jetzt gesetzt", eins.stand.hatBaseline === true);

  // Zweiter Abruf, unveraendert.
  const zwei = diffOccupiedCourts(eins.stand, [feld(1, 100), feld(2, 200)], "");
  pruefe("unveraenderter Stand sagt nichts an", zwei.neue.length === 0);

  // Jetzt kommt ein neues Spiel auf Feld 1 — DAS wird angesagt.
  const drei = diffOccupiedCourts(zwei.stand, [feld(1, 101), feld(2, 200)], "");
  pruefe("neues Spiel auf Feld 1 wird angesagt", drei.neue.length === 1);
  pruefe("und zwar das richtige", drei.neue[0]?.match_id === 101);
}

console.log("\n=== Der gemeldete Fehler: erster Abruf noch ohne BTP-Daten ===");
{
  // Genau die Reihenfolge beim Start: Der Sync-Lauf hat noch keinen
  // Schnappschuss, `tablet_overview` liefert eine LEERE Liste.
  let stand = { baseline: new Map(), hatBaseline: false };
  const leer = diffOccupiedCourts(stand, [], "");
  pruefe("leerer Abruf sagt nichts an", leer.neue.length === 0);
  pruefe(
    "ein leerer Stand taugt NICHT als Baseline (sonst gilt gleich alles als neu)",
    leer.stand.hatBaseline === false,
  );

  // Zwei Sekunden spaeter sind die zwoelf laufenden Spiele da.
  const voll = diffOccupiedCourts(
    leer.stand,
    [feld(1, 100), feld(2, 200), feld(3, 300)],
    "",
  );
  pruefe("kein Spiel wird angesagt (das war der Fehler)", voll.neue.length === 0);
  pruefe("erst jetzt steht die Baseline", voll.stand.hatBaseline === true);

  // Und ab hier wird wieder normal angesagt.
  const neu = diffOccupiedCourts(
    voll.stand,
    [feld(1, 100), feld(2, 201), feld(3, 300)],
    "",
  );
  pruefe("danach wird ein echter Aufruf angesagt", neu.neue.length === 1);
  pruefe("und zwar der richtige", neu.neue[0]?.match_id === 201);
}

console.log("\n=== Turnierstart: alle Felder frei ===");
{
  // Die Felder existieren, es laeuft nur noch nichts. Das ist ein
  // brauchbarer Anfangsstand — sonst bliebe der erste Aufruf des Tages stumm.
  let stand = { baseline: new Map(), hatBaseline: false };
  const frei = diffOccupiedCourts(stand, [feld(1, 0), feld(2, 0)], "");
  pruefe("freie Felder taugen als Baseline", frei.stand.hatBaseline === true);
  pruefe("angesagt wird nichts", frei.neue.length === 0);

  const erster = diffOccupiedCourts(frei.stand, [feld(1, 100), feld(2, 0)], "");
  pruefe("der erste Aufruf des Tages wird angesagt", erster.neue.length === 1);
}

console.log("\n=== Hallenfilter ===");
{
  let stand = { baseline: new Map(), hatBaseline: false };
  const start = diffOccupiedCourts(
    stand,
    [feld(1, 0, "Halle A"), feld(2, 0, "Halle B")],
    "Halle A",
  );
  const beide = diffOccupiedCourts(
    start.stand,
    [feld(1, 100, "Halle A"), feld(2, 200, "Halle B")],
    "Halle A",
  );
  pruefe("nur die eigene Halle wird angesagt", beide.neue.length === 1);
  pruefe("und zwar Halle A", beide.neue[0]?.match_id === 100);
  pruefe(
    "die fremde Halle ist trotzdem in der Baseline (kein Nachholen beim Umschalten)",
    beide.stand.baseline.get(2) === 200,
  );
}

console.log("\n=== Feld wird frei und neu belegt ===");
{
  let stand = { baseline: new Map(), hatBaseline: false };
  const a = diffOccupiedCourts(stand, [feld(1, 100)], "");
  const b = diffOccupiedCourts(a.stand, [feld(1, 0)], "");
  pruefe("freiwerdendes Feld sagt nichts an", b.neue.length === 0);
  const c = diffOccupiedCourts(b.stand, [feld(1, 101)], "");
  pruefe("das naechste Spiel darauf wird angesagt", c.neue.length === 1);
}

console.log("\n=== Halle hinzunehmen holt nichts nach ===");
{
  // Slave-Befund 23.08.2026: Nimmt man am Ansage-Slave eine Halle hinzu,
  // wurden schlagartig viele Ansagen aus der Vergangenheit wiederholt. Der
  // Nutzer will genau das nicht — ab sofort ansagen reicht.
  //
  // Das Modul kann das schon: Die Baseline merkt sich auch Felder der
  // gefilterten Halle (siehe Test oben). Diese Tests halten die Eigenschaft
  // fest, auf die sich der MatchAnnouncer stuetzt, seit er die Baseline beim
  // Hallenwechsel NICHT mehr leert.
  let stand = { baseline: new Map(), hatBaseline: false };
  // Nur Halle A wird angesagt; in Halle B laufen laengst Spiele.
  const start = diffOccupiedCourts(
    stand,
    [feld(1, 100, "Halle A"), feld(2, 200, "Halle B"), feld(3, 300, "Halle B")],
    "Halle A",
  );
  const lauf = diffOccupiedCourts(
    start.stand,
    [feld(1, 100, "Halle A"), feld(2, 200, "Halle B"), feld(3, 300, "Halle B")],
    "Halle A",
  );
  pruefe("waehrenddessen wird nichts aus Halle B angesagt", lauf.neue.length === 0);

  // Jetzt schaltet der Slave auf „alle Hallen" um — derselbe Stand, nur ohne
  // Filter. Nichts davon darf nachgerufen werden.
  const umschalten = diffOccupiedCourts(
    lauf.stand,
    [feld(1, 100, "Halle A"), feld(2, 200, "Halle B"), feld(3, 300, "Halle B")],
    "",
  );
  pruefe(
    "beim Umschalten wird kein laufendes Spiel nachgerufen",
    umschalten.neue.length === 0,
  );

  // Ab sofort heisst: Das naechste ECHTE Spiel in der neuen Halle kommt.
  const danach = diffOccupiedCourts(
    umschalten.stand,
    [feld(1, 100, "Halle A"), feld(2, 201, "Halle B"), feld(3, 300, "Halle B")],
    "",
  );
  pruefe("ein neues Spiel in Halle B wird angesagt", danach.neue.length === 1);
  pruefe("und zwar das richtige", danach.neue[0]?.match_id === 201);
}

console.log("");
if (fehler > 0) {
  console.error(`${fehler} FEHLER`);
  process.exit(1);
}
console.log("ALLE TESTS GRUEN");
