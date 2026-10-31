// Testet die Entscheidungen der Ansage-Seite (src/io/ansageSeite.mjs) —
// das echte Modul, dessen Inline-Kopie assets/ansage.html trägt — und hält
// die Kopie wortgleich (Spec ansage-im-browser).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import {
  ANSAGE_TAKT_MS, ANSAGE_VERFALL_MS, hallePasst, halleNoetig, halleGueltig,
  neueEintraege, belegungPasst, nochAbspielbar, stimme, meldetPraesenz,
  grundText, feedAdresse, ANSAGE_VERLAUF_MAX, ansageText, verlaufAufnehmen,
  wiederholungsAuftrag, schriftSuchen,
} from "../src/io/ansageSeite.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

ok("Takt 1,5 s, Verfall 60 s", [ANSAGE_TAKT_MS, ANSAGE_VERFALL_MS], [1500, 60000]);

// ── Hallen ──────────────────────────────────────────────────────────────
ok("Halle: Schreibweise und Rand egal (Umlaut)", hallePasst("Halle Ü", " halle ü "), true);
ok("Halle: andere Halle", hallePasst("Halle A", "Halle B"), false);
ok("Halle: leeres Ziel gilt allen", hallePasst("", "Halle B"), true);
ok("eine Halle → keine Wahl nötig", halleNoetig(["Halle A"]), false);
ok("zwei Hallen → Wahl Pflicht", halleNoetig(["Halle A", "Halle B"]), true);
ok("Kriterium 14: ab zwei Hallen ohne Wahl kein Start", halleGueltig(["A", "B"], ""), false);
ok("ab zwei Hallen mit unbekannter Halle kein Start", halleGueltig(["A", "B"], "C"), false);
ok("ab zwei Hallen mit gültiger Halle", halleGueltig(["Halle A", "Halle B"], "halle b"), true);
ok("eine Halle: auch ohne Wahl", halleGueltig([], ""), true);

// ── Neue Einträge ───────────────────────────────────────────────────────
const e = (id, extra = {}) => ({ id, art: "court_call", segmente: ["x"], ...extra });
let r = neueEintraege({ lastId: 0, bestand: false }, { eintraege: [e(5), e(7)] });
ok("erster Feed = Bestandsaufnahme, nichts spielen", r.neu, []);
ok("… aber die Nummer merken", r.stand, { lastId: 7, bestand: true });
r = neueEintraege(r.stand, { eintraege: [e(7), e(9), e(8)] });
ok("danach nur Neues, aufsteigend", r.neu.map((x) => x.id), [8, 9]);
r = neueEintraege(r.stand, { eintraege: [e(10, { art: "kommt_noch" }), e(11)] });
ok("unbekannte Art wird übersprungen", r.neu.map((x) => x.id), [11]);
ok("… die Nummer zieht trotzdem nach", r.stand.lastId, 11);
ok("kaputter Feed wirft nicht", neueEintraege({ lastId: 3, bestand: true }, null).neu, []);

// ── Belegung und Verfall ────────────────────────────────────────────────
const bel = [{ courtId: 3, matchId: 42 }];
ok("Eintrag ohne Spiel passt immer", belegungPasst(e(1), []), true);
ok("Kriterium 5: anderes Spiel auf dem Feld → schweigen",
  belegungPasst(e(1, { courtId: 3, matchId: 43 }), bel), false);
ok("richtiges Spiel → spielen", belegungPasst(e(1, { courtId: 3, matchId: 42 }), bel), true);
ok("frisch und richtig", nochAbspielbar(e(1), [], 1000, 1000 + 59999), true);
ok("Kriterium 6: nach einer Minute verfallen", nochAbspielbar(e(1), [], 1000, 1000 + 60000), false);

// ── Stimme und Präsenz ──────────────────────────────────────────────────
ok("Host-Audio vor Gerätestimme", stimme(true, true), "host");
ok("ohne Host-Audio die Gerätestimme", stimme(false, true), "geraet");
ok("ohne beides keine", stimme(false, false), "keine");
const aktiv = { aktiv: true, hostAudio: false };
ok("Kriterium 11: ohne Stimme keine Präsenz", meldetPraesenz(aktiv, false, true), false);
ok("ohne freigeschalteten Ton keine Präsenz", meldetPraesenz(aktiv, true, false), false);
ok("Feed nicht aktiv → keine Präsenz", meldetPraesenz({ aktiv: false, hostAudio: true }, true, true), false);
ok("aktiv + Stimme + Ton → Präsenz", meldetPraesenz(aktiv, true, true), true);
ok("ohne Feed keine Präsenz", meldetPraesenz(null, true, true), false);

ok("Grundtext Schalter nennt den Schalter", grundText("schalter_aus").includes("Audio für Ansage-Seiten erzeugen"), true);
ok("Grundtext unbekannt hat einen Text", grundText("xyz").length > 0, true);
ok("Feed-Adresse", feedAdresse("/", "Halle Ü", 7, true), "/ansage/feed?halle=Halle%20%C3%9C&since=7&hoert=1");
ok("Feed-Adresse ohne Präsenz", feedAdresse("/", "", 0, false), "/ansage/feed?halle=&since=0");

// ── Verlauf + Großanzeige (Erweiterung 30.10.2026) ──────────────────────
ok("Verlauf hält fünf", ANSAGE_VERLAUF_MAX, 5);
ok("Text aus Segmenten", ansageText({ segmente: [" Feld 3: ", "", null, "Anna  Muster\ngegen Bea"] }),
  "Feld 3: Anna Muster gegen Bea");
ok("ohne Segmente leer", ansageText({}), "");
{
  let v = [];
  for (let i = 1; i <= 7; i++) v = verlaufAufnehmen(v, { id: i, text: "A" + i });
  ok("Deckel 5, neueste vorn", v.map((x) => x.id), [7, 6, 5, 4, 3]);
  const gleich = { id: 5, text: "neu" };
  v = verlaufAufnehmen(v, gleich);
  ok("gleiche Ansage steht nie doppelt", v.map((x) => x.id), [5, 7, 6, 4, 3]);
  ok("Eintrag wird nicht kopiert (Audio landet an ihm)", v[0] === gleich, true);
  ok("eigener Deckel", verlaufAufnehmen(v, { id: 9 }, 2).map((x) => x.id), [9, 5]);
  const a = wiederholungsAuftrag(v, 6);
  ok("Wiederholungsauftrag trägt Ansage und Eintrag", [a.wiederholung, a.verlauf.id], [true, 6]);
  ok("unbekannte Ansage: kein Auftrag", wiederholungsAuftrag(v, 99), null);
}
ok("Schrift: größte passende", schriftSuchen(16, 400, (px) => px <= 137), 137);
ok("Schrift: alles passt → max", schriftSuchen(16, 400, () => true), 400);
ok("Schrift: nichts passt → min", schriftSuchen(16, 400, () => false), 16);
ok("Schrift: max unter min → min", schriftSuchen(16, 10, () => true), 16);

// ── Inline-Kopie in ansage.html ─────────────────────────────────────────
{
  const hier = dirname(fileURLToPath(import.meta.url));
  const modul = readFileSync(join(hier, "..", "src", "io", "ansageSeite.mjs"), "utf8");
  const html = readFileSync(join(hier, "..", "src-tauri", "assets", "ansage.html"), "utf8");
  const erwartet = modul.split("\n\n").slice(1).join("\n\n").replace(/^export /gm, "").trim();
  const m = html.match(/\/\* <ansageSeite\.mjs> \*\/\n([\s\S]*?)\n\/\* <\/ansageSeite\.mjs> \*\//);
  ok("ansage.html trägt die Kopie", !!m, true);
  ok("Kopie gleicht dem Modul", m && m[1].trim() === erwartet, true);
  ok("Seite fragt nur per GET (kein method:)", /method\s*:/.test(html), false);
  ok("Seite nutzt den Basis-Platzhalter", html.includes('var BASE = "__BASE__";'), true);
}

if (failures) {
  console.error(`${failures} Test(s) fehlgeschlagen`);
  process.exit(1);
}
console.log("Alle Ansage-Seiten-Tests grün.");
