// Testet das Mischen und Beschriften offener Paarungen
// (src/io/offeneSpiele.mjs, Spec tl-offene-paarungen, ADR 0051/0052) — das
// echte Modul, dessen Inline-Kopie tl.html trägt.
//
// Beide Regeln sind heikel: Mischt `mischeOffene` falsch, steht ein
// Viertelfinale mitten zwischen den Spielen, die als Nächstes drankommen —
// und die Turnierleitung arbeitet die Liste in der falschen Reihenfolge ab.
// Fällt `seitenText` zu früh auf ein Label zurück, verschwindet die Hälfte
// einer bekannten Paarung aus der Anzeige.
import {
  mischeOffene,
  istOffen,
  seitenText,
  OFFEN_TEXT,
} from "../src/io/offeneSpiele.mjs";

let failures = 0;
function ok(name, got, want) {
  const a = JSON.stringify(got);
  const b = JSON.stringify(want);
  if (a !== b) {
    console.error(`✗ ${name}: erwartet ${b}, war ${a}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

const echt = (id) => ({ match_id: id });
const offen = (id, index) => ({ match_id: id, queue_index: index });
const ids = (liste) => liste.map((e) => e.match_id);

// ── mischeOffene: die Position kommt vom Host ──

ok(
  "ohne offene Spiele bleibt die Liste, wie sie war",
  ids(mischeOffene([echt(1), echt(2)], [])),
  [1, 2],
);

ok(
  "ein alter Turnier-PC schickt gar keine offene Liste",
  ids(mischeOffene([echt(1), echt(2)], undefined)),
  [1, 2],
);

ok(
  "queue_index 0 heißt ganz vorn",
  ids(mischeOffene([echt(1), echt(2)], [offen(90, 0)])),
  [90, 1, 2],
);

ok(
  "queue_index 1 heißt hinter dem ersten Spiel",
  ids(mischeOffene([echt(1), echt(2)], [offen(90, 1)])),
  [1, 90, 2],
);

ok(
  "ein Index jenseits der Liste landet am Ende",
  ids(mischeOffene([echt(1), echt(2)], [offen(90, 99)])),
  [1, 2, 90],
);

ok(
  "mehrere offene Spiele reihen sich an ihren Stellen ein",
  ids(mischeOffene([echt(1), echt(2), echt(3)], [offen(90, 1), offen(91, 3)])),
  [1, 90, 2, 3, 91],
);

ok(
  "zwei offene an derselben Stelle behalten die Reihenfolge des Hosts",
  ids(mischeOffene([echt(1), echt(2)], [offen(90, 1), offen(91, 1)])),
  [1, 90, 91, 2],
);

ok(
  "eine leere Arbeitsliste zeigt nur die offenen Spiele",
  ids(mischeOffene([], [offen(90, 0), offen(91, 0)])),
  [90, 91],
);

// Ein fehlender oder unsinniger Index darf die Liste nicht zerreißen — der
// Eintrag landet dann an der aktuellen Stelle statt zu verschwinden.
ok(
  "ein Eintrag ohne Index geht nicht verloren",
  ids(mischeOffene([echt(1)], [{ match_id: 90 }])),
  [90, 1],
);

// ── istOffen: die Marke kommt aus dem Mischen ──

const gemischt = mischeOffene([echt(1)], [offen(90, 1)]);
ok("das echte Spiel ist nicht offen", istOffen(gemischt[0]), false);
ok("das eingemischte Spiel ist offen", istOffen(gemischt[1]), true);
ok("ein fehlender Eintrag ist nicht offen", istOffen(undefined), false);

// ── seitenText: Namen schlagen jedes Label ──

ok(
  "eine feststehende Seite zeigt ihre Namen",
  seitenText({ team1: ["Müller"], open_slot1_label: "aus Spiel 42" }, 1),
  "Müller",
);

ok(
  "ein Doppel wird mit Schrägstrich verbunden",
  seitenText({ team1: ["Müller", "Meier"] }, 1),
  "Müller / Meier",
);

ok(
  "eine offene Seite zeigt das Label des Hosts",
  seitenText({ team2: [], open_slot2_label: "Weber oder Fischer" }, 2),
  "Weber oder Fischer",
);

ok(
  "ohne Namen und ohne Label bleibt es bei noch offen",
  seitenText({ team1: [] }, 1),
  OFFEN_TEXT,
);

ok(
  "ein leeres Label zählt nicht als Aussage",
  seitenText({ team1: [], open_slot1_label: "   " }, 1),
  OFFEN_TEXT,
);

if (failures > 0) {
  console.error(`\n${failures} Test(s) fehlgeschlagen.`);
  process.exit(1);
}
console.log("\nAlle Tests bestanden.");
