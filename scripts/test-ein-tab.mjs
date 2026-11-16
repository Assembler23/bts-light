// Testet die Absprache „ein Tab statt zwei" der Turnierleitungs-Oberfläche
// (src/io/einTab.mjs) und vergleicht die Inline-Kopie in tl.html auf
// denselben Eingaben. Eine falsche Regel schließt im schlimmsten Fall den
// Tab, in dem die Turnierleitung gerade ein Ergebnis eintippt.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  EIN_TAB_FRIST_MS, EIN_TAB_KANAL, einTabSchritt, vonAppGeoeffnet,
} from "../src/io/einTab.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

// ── Marke im Fragment ──
ok("Adresse des Knopfs trägt die Marke", vonAppGeoeffnet("#t=abc&app=1"), true);
ok("Marke vor dem Zugang", vonAppGeoeffnet("#app=1&t=abc"), true);
ok("Marke allein", vonAppGeoeffnet("#app=1"), true);
ok("QR-Adresse ohne Marke", vonAppGeoeffnet("#t=abc"), false);
ok("kein Fragment", vonAppGeoeffnet(""), false);
ok("undefined", vonAppGeoeffnet(undefined), false);
ok("Marke als Teil des Zugangs zählt nicht", vonAppGeoeffnet("#t=xapp=1"), false);
ok("app=10 ist nicht die Marke", vonAppGeoeffnet("#t=abc&app=10"), false);
ok("app=0 ist nicht die Marke", vonAppGeoeffnet("#t=abc&app=0"), false);

// ── Schon offener Tab hört „neu" ──
const T = 1_000_000;
const neu = (extra = {}) => ({ typ: "neu", von: "B", um: T, ...extra });
const alt = (extra = {}) => ({
  ich: "A", jetzt: T + 50, sichtbar: false, beschaeftigt: false, gemeldetUm: 0, ...extra,
});
ok("verdeckt, unbeschäftigt → geht", einTabSchritt(neu(), alt()), "schliessen");
ok("sichtbar → bleibt und meldet es", einTabSchritt(neu(), alt({ sichtbar: true })), "melden");
ok("sichtbar und beschäftigt → bleibt und meldet es",
  einTabSchritt(neu(), alt({ sichtbar: true, beschaeftigt: true })), "melden");
ok("verdeckt, Dialog offen → nichts (keine Eingabe verlieren)",
  einTabSchritt(neu(), alt({ beschaeftigt: true })), "nichts");
ok("eigene Meldung → nichts", einTabSchritt(neu({ von: "A" }), alt()), "nichts");
ok("Meldung ohne Absender → nichts", einTabSchritt(neu({ von: "" }), alt()), "nichts");
ok("Meldung an der Frist zählt noch",
  einTabSchritt(neu(), alt({ jetzt: T + EIN_TAB_FRIST_MS })), "schliessen");
ok("alte Meldung (Tab hat geschlafen) → nichts",
  einTabSchritt(neu(), alt({ jetzt: T + EIN_TAB_FRIST_MS + 1 })), "nichts");
ok("alte Meldung, inzwischen sichtbar → nichts",
  einTabSchritt(neu(), alt({ jetzt: T + 60_000, sichtbar: true })), "nichts");
ok("Meldung ohne Zeit → nichts", einTabSchritt(neu({ um: undefined }), alt()), "nichts");
ok("Meldung aus ferner Zukunft → nichts",
  einTabSchritt(neu({ um: T + 60_000 }), alt()), "nichts");

// ── Neuer Tab hört „bleibe" ──
const bleibe = (extra = {}) => ({ typ: "bleibe", von: "A", an: "B", ...extra });
const frisch = (extra = {}) => ({
  ich: "B", jetzt: T + 80, sichtbar: true, beschaeftigt: false, gemeldetUm: T, ...extra,
});
ok("Antwort auf die eigene Meldung → geht", einTabSchritt(bleibe(), frisch()), "schliessen");
ok("Antwort an einen anderen → nichts", einTabSchritt(bleibe({ an: "C" }), frisch()), "nichts");
ok("nie gemeldet (QR-Tab) → nichts",
  einTabSchritt(bleibe(), frisch({ gemeldetUm: 0 })), "nichts");
ok("späte Antwort → nichts (hier wird längst gearbeitet)",
  einTabSchritt(bleibe(), frisch({ jetzt: T + EIN_TAB_FRIST_MS + 1 })), "nichts");

// ── Müll auf dem Kanal ──
for (const m of [undefined, null, "neu", 7, {}, { typ: "weg", von: "B" }, { typ: "neu" }]) {
  ok(`Müll ${JSON.stringify(m)} → nichts`, einTabSchritt(m, alt({ sichtbar: true })), "nichts");
}
ok("ohne Lage → nichts", einTabSchritt(neu(), undefined), "nichts");

// ── Inline-Kopie in tl.html ──
function funktion(text, name) {
  const start = text.indexOf("function " + name + "(");
  if (start < 0) return null;
  let tiefe = 0;
  for (let i = text.indexOf("{", start); i < text.length; i++) {
    if (text[i] === "{") tiefe++;
    else if (text[i] === "}" && --tiefe === 0) return text.slice(start, i + 1);
  }
  return null;
}
const hier = dirname(fileURLToPath(import.meta.url));
const tl = readFileSync(join(hier, "..", "src-tauri", "assets", "tl.html"), "utf8");
const qMarke = funktion(tl, "vonAppGeoeffnet");
const qSchritt = funktion(tl, "einTabSchritt");
ok("tl.html trägt beide Kopien", !!qMarke && !!qSchritt, true);
if (qMarke && qSchritt) {
  // eslint-disable-next-line no-new-func
  const kopie = new Function(
    `const EIN_TAB_FRIST_MS = ${EIN_TAB_FRIST_MS};\n${qMarke}\n${qSchritt}\n` +
    "return { vonAppGeoeffnet, einTabSchritt };")();
  const hashes = [undefined, "", "#", "#t=a", "#t=a&app=1", "#app=1", "#app=1&t=a",
    "#t=aapp=1", "#t=a&app=10", "#t=a&app=0", "#t=a&app=1&x=2"];
  ok("Kopie liest die Marke wie das Modul",
    hashes.map((h) => kopie.vonAppGeoeffnet(h)), hashes.map((h) => vonAppGeoeffnet(h)));

  const nachrichten = [undefined, null, {}, { typ: "neu" }];
  for (const typ of ["neu", "bleibe", "weg"])
    for (const von of ["A", "B", ""])
      for (const an of ["A", "B", undefined])
        for (const um of [undefined, T, T - 6000, T + 6000])
          nachrichten.push({ typ, von, an, um });
  const lagen = [undefined];
  for (const ich of ["A", "B"])
    for (const sichtbar of [true, false])
      for (const beschaeftigt of [true, false])
        for (const gemeldetUm of [0, T, T - 6000])
          for (const jetzt of [T, T + 100, T + 5000, T + 5001])
            lagen.push({ ich, jetzt, sichtbar, beschaeftigt, gemeldetUm });
  const lauf = (f) => nachrichten.map((n) => lagen.map((l) => f(n, l)));
  ok("Kopie entscheidet wie das Modul", lauf(kopie.einTabSchritt), lauf(einTabSchritt));

  // Die Konstanten müssen in der Seite dieselben sein.
  ok("Seite nennt denselben Kanal",
    tl.includes(`const EIN_TAB_KANAL = "${EIN_TAB_KANAL}";`), true);
  ok("Seite nennt dieselbe Frist",
    tl.includes(`const EIN_TAB_FRIST_MS = ${EIN_TAB_FRIST_MS};`), true);
  // Die Marke muss gelesen sein, BEVOR der Zugang das Fragment wegräumt.
  const marke = tl.indexOf("vonAppGeoeffnet(location.hash)");
  const zugang = tl.indexOf("const token = readToken();");
  ok("Marke wird vor dem Aufräumen des Fragments gelesen",
    marke > 0 && zugang > 0 && marke < zugang, true);
}

// ── Wann gilt ein Tab als beschäftigt? ──
// Die Prüfung hängt am Zustand der Seite und lebt deshalb nur in tl.html.
// Hier läuft der ECHTE Funktionstext gegen eine nachgestellte Seite: Alles,
// was beim Schließen verloren ginge, muss den Tab offen halten.
const qBeschaeftigt = funktion(tl, "einTabBeschaeftigt");
ok("tl.html trägt die Beschäftigt-Prüfung", !!qBeschaeftigt, true);
if (qBeschaeftigt) {
  const feld = (art, wert) => ({
    value: wert,
    matches: () => art === "text" || art === "search" || art === "number" || art === "textarea",
  });
  const beschaeftigt = ({ offen = null, picked = null, pending = null, zug = false, aktiv = null } = {}) => {
    let gefragt = "";
    const document = {
      querySelector: (s) => { gefragt = s; return offen; },
      activeElement: aktiv,
    };
    // eslint-disable-next-line no-new-func
    const erg = new Function("document", "picked", "pending", "reorderDragPending",
      `${qBeschaeftigt}\nreturn einTabBeschaeftigt();`)(document, picked, pending, zug);
    return { erg, gefragt };
  };
  ok("leere Seite → frei", beschaeftigt().erg, false);
  ok("Dialog oder Überlagerung offen → beschäftigt", beschaeftigt({ offen: {} }).erg, true);
  const { gefragt } = beschaeftigt();
  ok("gefragt wird nach Dialogen", gefragt.includes("dialog[open]"), true);
  ok("… nach den Überlagerungen", gefragt.includes(".graph-overlay:not([hidden])"), true);
  ok("… und nach der Trefferliste der Suche", gefragt.includes(".such-overlay:not([hidden])"), true);
  ok("gewählte Partie wartet auf ihr Feld → beschäftigt",
    beschaeftigt({ picked: { match: {}, fromCourtId: null } }).erg, true);
  ok("Auftrag unterwegs → beschäftigt",
    beschaeftigt({ pending: { matchId: 1, courtId: 2 } }).erg, true);
  ok("Zug in der Liste läuft → beschäftigt", beschaeftigt({ zug: true }).erg, true);
  ok("Zeiger im Suchfeld mit Text → beschäftigt",
    beschaeftigt({ aktiv: feld("search", "Mül") }).erg, true);
  ok("Zeiger im leeren Suchfeld → frei", beschaeftigt({ aktiv: feld("search", "") }).erg, false);
  ok("Häkchen im Fokus zählt nicht (sein Wert ist immer „on“)",
    beschaeftigt({ aktiv: feld("checkbox", "on") }).erg, false);
  ok("Fokus auf einem Knopf → frei",
    beschaeftigt({ aktiv: { matches: () => false } }).erg, false);
  // Die Seite muss die Dinge auch so nennen, wie die Prüfung sie anspricht.
  ok("Trefferliste trägt die Klasse", /class="such-overlay"[^>]*\bhidden\b/.test(tl), true);
  ok("Auswahl heißt `picked`", /\nlet picked = null;/.test(tl), true);
  ok("Auftrag heißt `pending`", /\nlet pending = null;/.test(tl), true);
  ok("Zug heißt `reorderDragPending`", /\nlet reorderDragPending = false;/.test(tl), true);
}

if (failures) {
  console.error(`\n${failures} Fehler`);
  process.exit(1);
}
console.log("\nalle Tests grün");
