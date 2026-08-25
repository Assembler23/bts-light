// Testet die zwei Entscheidungen der Kombi-Anzeige (src/io/comboAnsicht.mjs,
// Spec kombi-ausrichtung-je-monitor, ADR 0049) — das echte Modul, dessen
// Inline-Kopie combo.html trägt.
//
// Beide Regeln sind heikel: `urlPasst` zu streng → die Seite navigiert im
// Sekundentakt neu (der Fehler, den dieses Feature mitfixt); zu großzügig →
// eine echte Umzuweisung kommt nie an. `ausrichtungVertikal` zu großzügig →
// eine hand-getippte Kiosk-URL verliert ihren Startwert.
import { urlPasst, ausrichtungVertikal } from "../src/io/comboAnsicht.mjs";

let failures = 0;
function ok(name, got, want) {
  if (got !== want) {
    console.error(`✗ ${name}: erwartet ${want}, war ${got}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

const O = "http://bts-light.local:8088";

// ── urlPasst: welche Query-Parameter zählen als Unterschied? ──

ok(
  "identische URL passt",
  urlPasst(`${O}/combo?courts=1,2,3`, "/combo?courts=1,2,3", O),
  true,
);

ok(
  "device zählt nicht als Unterschied",
  urlPasst(`${O}/combo?courts=1,2,3&device=abc`, "/combo?courts=1,2,3", O),
  true,
);

// Der eigentliche Bug: navigateWithRotate hängt `rotate` wieder an, das Ziel
// trägt es nie — ohne diese Zeile navigiert ein hochkant montierter Kombi-TV
// im Sekundentakt neu (AK6).
ok(
  "rotate zählt nicht als Unterschied (AK6)",
  urlPasst(
    `${O}/combo?courts=1,2,3&device=abc&rotate=90`,
    "/combo?courts=1,2,3",
    O,
  ),
  true,
);

// Beim Update baut der Server kein `&dir=v` mehr. Ohne diese Zeile lüde
// ausgerechnet jeder migrierte TV einmal komplett neu — schwarzes Bild im
// laufenden Spiel.
ok(
  "dir zählt nicht als Unterschied (Übergang beim Update)",
  urlPasst(`${O}/combo?courts=1,2,3&dir=v&device=abc`, "/combo?courts=1,2,3", O),
  true,
);

ok(
  "alle drei zusammen",
  urlPasst(
    `${O}/combo?courts=1,2,3&dir=v&device=abc&rotate=270`,
    "/combo?courts=1,2,3",
    O,
  ),
  true,
);

// Die Gegenprobe: Der Fix darf keine echte Umzuweisung verschlucken.
ok(
  "andere Felder sind ein echter Unterschied",
  urlPasst(`${O}/combo?courts=1,2&device=abc`, "/combo?courts=1,2,3", O),
  false,
);

ok(
  "andere Reihenfolge der Felder ist ein Unterschied",
  urlPasst(`${O}/combo?courts=2,1&device=abc`, "/combo?courts=1,2", O),
  false,
);

ok(
  "anderer Pfad ist ein Unterschied",
  urlPasst(`${O}/combo?courts=1,2`, "/monitor", O),
  false,
);

ok(
  "kaputtes Ziel gilt als Unterschied (dann lieber navigieren)",
  urlPasst(`${O}/combo?courts=1,2`, "://kaputt", O),
  false,
);

// ── ausrichtungVertikal: Server schlägt Startwert, aber nur wenn er redet ──

ok(
  "Server sagt nebeneinander",
  ausrichtungVertikal({ vertical: true }, false),
  true,
);

ok(
  "Server sagt übereinander — schlägt den Startwert",
  ausrichtungVertikal({ vertical: false }, true),
  false,
);

// AK7: Ohne `?device=` kennt der Host die Seite nicht und schickt `null`.
// Ein pauschales `false` kippte sie auf „übereinander".
ok(
  "null lässt den Startwert stehen (AK7)",
  ausrichtungVertikal({ vertical: null }, true),
  true,
);

ok(
  "fehlendes Feld lässt den Startwert stehen (alter Server)",
  ausrichtungVertikal({ courts: [] }, true),
  true,
);

ok(
  "kein State lässt den Startwert stehen",
  ausrichtungVertikal(null, true),
  true,
);

// Nur ein echter Boolean zählt — sonst machte ein Server-Fehler ("", 0, "v")
// aus der Ausrichtung ein Zufallsergebnis.
ok(
  "String zählt nicht als Ansage",
  ausrichtungVertikal({ vertical: "v" }, true),
  true,
);

ok("Startwert übereinander bleibt", ausrichtungVertikal({}, false), false);

if (failures > 0) {
  console.error(`\n${failures} Fall/Fälle fehlgeschlagen.`);
  process.exit(1);
}
console.log("\nAlle Fälle bestanden.");
