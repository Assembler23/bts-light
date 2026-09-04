// Testet Ziel-Allowlist und Pfadbau der Anzeige-Hülle
// (src/io/anzeigeZiel.mjs, Spec zaehltafel-anzeige-huelle) — das echte Modul,
// dessen Inline-Kopie anzeige.html trägt. Hier entscheidet sich, was ins
// iframe-src darf: alles, was hier durchrutscht, lädt die Hülle.
import { LAYOUTS, feldbezogen, zielAusQuery, zielPfad } from "../src/io/anzeigeZiel.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

// ── Allowlist ────────────────────────────────────────────────────────────
ok("vier Layouts", LAYOUTS, ["tafel", "feld", "uebersicht", "vorbereitung"]);
for (const l of LAYOUTS) ok(`bekanntes Layout ${l} bleibt`, zielAusQuery(l, "3").layout, l);
ok("unbekanntes Layout → tafel", zielAusQuery("monitor", "3").layout, "tafel");
ok("leeres Layout → tafel", zielAusQuery("", "3").layout, "tafel");
ok("fehlendes Layout → tafel", zielAusQuery(null, "3").layout, "tafel");
ok("Pfad-Einschleusung → tafel", zielAusQuery("../tl", "3").layout, "tafel");
ok("javascript: → tafel", zielAusQuery("javascript:alert(1)", "3").layout, "tafel");
ok("Layout mit Query → tafel", zielAusQuery("tafel?x=1", "3").layout, "tafel");
ok("Groß-/Kleinschreibung zählt", zielAusQuery("Tafel", "3").layout, "tafel");
ok("feldbezogen: tafel/feld ja", [feldbezogen("tafel"), feldbezogen("feld")], [true, true]);
ok("feldbezogen: uebersicht/vorbereitung nein", [feldbezogen("uebersicht"), feldbezogen("vorbereitung")], [false, false]);

// ── CourtID: nur positive Ganzzahl ───────────────────────────────────────
ok("court 3", zielAusQuery("tafel", "3").court, 3);
ok("court 101", zielAusQuery("tafel", "101").court, 101);
ok("court abc → null", zielAusQuery("tafel", "abc").court, null);
ok("court -1 → null", zielAusQuery("tafel", "-1").court, null);
ok("court 0 → null", zielAusQuery("tafel", "0").court, null);
ok("court 3.5 → null", zielAusQuery("tafel", "3.5").court, null);
ok("court 1e3 → null", zielAusQuery("tafel", "1e3").court, null);
ok("court mit Pfad → null", zielAusQuery("tafel", "3/../../tl").court, null);
ok("court mit Leerzeichen → null", zielAusQuery("tafel", " 3").court, null);
ok("court fehlt → null", zielAusQuery("tafel", null).court, null);
ok("court leer → null", zielAusQuery("tafel", "").court, null);
ok("court zu lang → null", zielAusQuery("tafel", "12345678901").court, null);

// ── Pfadbau ──────────────────────────────────────────────────────────────
ok("tafel ohne Spiegel", zielPfad({ layout: "tafel", court: 3 }, false), "court/3/tafel");
ok("tafel mit Spiegel", zielPfad({ layout: "tafel", court: 3 }, true), "court/3/tafel?spiegel=1");
ok("feld ignoriert Spiegel", zielPfad({ layout: "feld", court: 3 }, true), "court/3/display");
ok("uebersicht ohne Feld", zielPfad({ layout: "uebersicht", court: null }, false), "info/overview");
ok("uebersicht ignoriert Feld", zielPfad({ layout: "uebersicht", court: 3 }, true), "info/overview");
ok("vorbereitung", zielPfad({ layout: "vorbereitung", court: null }, false), "info/preparation");
ok("tafel ohne Feld → null", zielPfad({ layout: "tafel", court: null }, false), null);
ok("feld ohne Feld → null", zielPfad({ layout: "feld", court: null }, false), null);
ok("unbekanntes Layout → null", zielPfad({ layout: "x", court: 3 }, false), null);
ok("kein Pfad beginnt mit /", LAYOUTS.every((l) => !String(zielPfad({ layout: l, court: 3 }, false)).startsWith("/")), true);

if (failures > 0) { console.error(`${failures} Fehler`); process.exit(1); }
console.log("alle Anzeige-Ziel-Tests grün");
