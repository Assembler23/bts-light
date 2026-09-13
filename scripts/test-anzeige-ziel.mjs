// Testet Ziel-Allowlist und Pfadbau der Anzeige-Hülle
// (src/io/anzeigeZiel.mjs, Spec zaehltafel-anzeige-huelle) — das echte Modul,
// dessen Inline-Kopie anzeige.html trägt. Hier entscheidet sich, was ins
// iframe-src darf: alles, was hier durchrutscht, lädt die Hülle.
import {
  LAYOUTS, feldbezogen, zielAusQuery, zielPfad,
  ANSICHTEN, ANSICHT_LABEL, ansichtAusWert, ansichtParameter, ansichtAusParametern, naechsteAnsicht,
} from "../src/io/anzeigeZiel.mjs";

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

// ── Anordnung (nur Zähltafel): Allowlist, `auto` schreibt nichts ─────────
ok("tafel mit Anordnung übereinander", zielPfad({ layout: "tafel", court: 3 }, false, "uebereinander"), "court/3/tafel?anordnung=uebereinander");
ok("tafel mit Anordnung nebeneinander", zielPfad({ layout: "tafel", court: 3 }, false, "nebeneinander"), "court/3/tafel?anordnung=nebeneinander");
ok("tafel: auto lässt die Adresse frei", zielPfad({ layout: "tafel", court: 3 }, false, "auto"), "court/3/tafel");
ok("tafel: Spiegel und Anordnung zusammen", zielPfad({ layout: "tafel", court: 3 }, true, "uebereinander"), "court/3/tafel?spiegel=1&anordnung=uebereinander");
ok("tafel: Unfug in der Anordnung erreicht die Adresse nie", zielPfad({ layout: "tafel", court: 3 }, false, "x&y=1"), "court/3/tafel");
ok("feld ignoriert Anordnung", zielPfad({ layout: "feld", court: 3 }, false, "uebereinander"), "court/3/display");

// ── Ansicht der Zähltafel: ein Zyklus für Spiegel + Anordnung ────────────
// Ein Tipp auf die Zahlen schaltet reihum; Menü und Tipp teilen sich diese
// Folge. „auto + gespiegelt" gibt es bewusst nicht mehr.
ok("fünf Ansichten in Reihenfolge", ANSICHTEN, ["auto", "links-rechts", "rechts-links", "oben-unten", "unten-oben"]);
ok("jede Ansicht hat ein Etikett", ANSICHTEN.every((a) => typeof ANSICHT_LABEL[a] === "string" && ANSICHT_LABEL[a].length > 0), true);
ok("Etikett links/rechts", ANSICHT_LABEL["links-rechts"], "links/rechts");
ok("Etikett automatisch", ANSICHT_LABEL.auto, "automatisch");

ok("auto → Automatik, ungespiegelt", ansichtParameter("auto"), { spiegel: false, anordnung: "auto" });
ok("links/rechts → nebeneinander, ungespiegelt", ansichtParameter("links-rechts"), { spiegel: false, anordnung: "nebeneinander" });
ok("rechts/links → nebeneinander, gespiegelt", ansichtParameter("rechts-links"), { spiegel: true, anordnung: "nebeneinander" });
ok("oben/unten → übereinander, gespiegelt", ansichtParameter("oben-unten"), { spiegel: true, anordnung: "uebereinander" });
ok("unten/oben → übereinander, ungespiegelt (heutiges Verhalten)", ansichtParameter("unten-oben"), { spiegel: false, anordnung: "uebereinander" });
ok("Unfug → wie auto", ansichtParameter("x"), { spiegel: false, anordnung: "auto" });

ok("Wert gültig bleibt", ansichtAusWert("oben-unten"), "oben-unten");
ok("Wert unbekannt → auto", ansichtAusWert("kopfueber"), "auto");
ok("Wert fehlt → auto", ansichtAusWert(null), "auto");
ok("Wert kein String → auto", ansichtAusWert(3), "auto");

// Reihum, am Ende wieder von vorn; Unbekanntes startet bei der ersten
// Hand-Ansicht (als käme es von „auto").
ok("nach auto kommt links/rechts", naechsteAnsicht("auto"), "links-rechts");
ok("nach unten/oben kommt auto", naechsteAnsicht("unten-oben"), "auto");
ok("Zyklus schließt sich nach fünf Schritten", (() => { let a = "auto"; for (let i = 0; i < 5; i++) a = naechsteAnsicht(a); return a; })(), "auto");
ok("nach Unfug kommt links/rechts", naechsteAnsicht("x"), "links-rechts");

// Migration der alten Schlüssel (Spiegel-Häkchen + Anordnung) — einmalig.
ok("alt: aus + auto → auto", ansichtAusParametern(false, "auto"), "auto");
ok("alt: an + auto → auto (Kombination entfällt)", ansichtAusParametern(true, "auto"), "auto");
ok("alt: aus + nebeneinander → links/rechts", ansichtAusParametern(false, "nebeneinander"), "links-rechts");
ok("alt: an + nebeneinander → rechts/links", ansichtAusParametern(true, "nebeneinander"), "rechts-links");
ok("alt: aus + übereinander → unten/oben", ansichtAusParametern(false, "uebereinander"), "unten-oben");
ok("alt: an + übereinander → oben/unten", ansichtAusParametern(true, "uebereinander"), "oben-unten");
ok("alt: Unfug → auto", ansichtAusParametern(true, "x"), "auto");
ok("Hin und zurück ist eindeutig", ANSICHTEN.every((a) => { const p = ansichtParameter(a); return ansichtAusParametern(p.spiegel, p.anordnung) === a; }), true);

// Der Pfadbau versteht die Parameter jeder Ansicht.
ok("Pfad für oben/unten", (() => { const p = ansichtParameter("oben-unten"); return zielPfad({ layout: "tafel", court: 3 }, p.spiegel, p.anordnung); })(), "court/3/tafel?spiegel=1&anordnung=uebereinander");
ok("Pfad für auto", (() => { const p = ansichtParameter("auto"); return zielPfad({ layout: "tafel", court: 3 }, p.spiegel, p.anordnung); })(), "court/3/tafel");

if (failures > 0) { console.error(`${failures} Fehler`); process.exit(1); }
console.log("alle Anzeige-Ziel-Tests grün");
