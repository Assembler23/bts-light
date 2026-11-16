// Sprachumschalter der Tablet-Seite (Deutsch / English).
//
// Die Seite führt ein Wörterbuch `EN` (Schlüssel = deutscher Text) und holt
// jeden Bedientext über `t('…')` bzw. `data-t` am Element. Ein Text, der im
// Wörterbuch fehlt, bliebe im englischen Betrieb still deutsch — genau die
// gemischte Zweisprachigkeit, die der Umschalter ablöst. Geprüft wird am
// echten Seitenquelltext:
//   1. jeder Schlüssel (Literal, `data-t`, dynamische Quelle) hat einen Eintrag,
//   2. kein Eintrag ist verwaist, Platzhalter stimmen in beiden Sprachen,
//   3. die alten Mischtexte („Deutsch · English") sind weg,
//   4. `t` und die Hüllen um die Inline-Kopien verhalten sich in beiden Sprachen.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const seite = readFileSync(new URL("../src-tauri/assets/tablet.html", import.meta.url), "utf8");

// ── Wörterbuch ausschneiden ──────────────────────────────────────────────
const enStart = seite.indexOf("const EN = {");
assert.ok(enStart >= 0, "tablet.html: Wörterbuch `const EN` fehlt");
const enEnde = seite.indexOf("\n};", enStart);
assert.ok(enEnde > enStart, "tablet.html: Wörterbuch `const EN` endet nicht");
const enQuelle = seite.slice(enStart + "const EN = ".length, enEnde + 2);
const EN = new Function("return " + enQuelle)();
const ohneWoerterbuch = seite.slice(0, enStart) + seite.slice(enEnde + 3);

// Doppelte Schlüssel im Objektliteral überschreiben sich still — der zweite
// gewinnt, und eine Korrektur am ersten bliebe wirkungslos.
{
  const gesehen = new Set();
  // Ein Eintrag beginnt am Zeilenanfang oder hinter einem Komma (kurze Paare
  // wie 'Links'/'Rechts' teilen sich eine Zeile).
  for (const m of enQuelle.matchAll(/(?:^|,)[ \t]*'((?:[^'\\]|\\.)*)':\s/gm)) {
    assert.ok(!gesehen.has(m[1]), `Wörterbuch: Schlüssel doppelt — „${m[1]}"`);
    gesehen.add(m[1]);
  }
  assert.equal(gesehen.size, Object.keys(EN).length, "Wörterbuch: die Doppel-Prüfung sieht jeden Eintrag");
}

function funktion(name) {
  const start = seite.indexOf("function " + name + "(");
  assert.ok(start >= 0, `tablet.html: function ${name} fehlt`);
  let tiefe = 0;
  for (let i = seite.indexOf("{", start); i < seite.length; i++) {
    if (seite[i] === "{") tiefe++;
    else if (seite[i] === "}" && --tiefe === 0) return seite.slice(start, i + 1);
  }
  throw new Error(`tablet.html: function ${name} endet nicht`);
}
const literal = (s) => new Function("return '" + s + "'")();

// ── Schlüssel einsammeln ─────────────────────────────────────────────────
const schluessel = new Map(); // Text → Herkunft (für die Fehlermeldung)
const merke = (text, woher) => { if (!schluessel.has(text)) schluessel.set(text, woher); };

// (a) Literale: t('…')
for (const m of ohneWoerterbuch.matchAll(/(?<![\w.$])t\('((?:[^'\\\n]|\\.)*)'/g)) merke(literal(m[1]), "t('…')");

// (b) Feste Blatt-Texte: <tag … data-t>Text</tag>
const entkomme = (s) => s.replace(/&amp;/g, "&").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"');
let blattTexte = 0;
for (const m of seite.matchAll(/<([a-z0-9]+)\b[^>]*?\sdata-t(?=[\s>])[^>]*>([\s\S]*?)<\/\1>/g)) {
  assert.ok(!m[2].includes("<"), `data-t nur an Blatt-Elementen ohne Kinder — sonst frisst textContent sie: ${m[0].slice(0, 80)}`);
  const text = entkomme(m[2]).replace(/\s+/g, " ").trim();
  assert.ok(text, `data-t an einem leeren Element: ${m[0].slice(0, 80)}`);
  merke(text, "data-t");
  blattTexte++;
}
assert.ok(blattTexte >= 60, `nur ${blattTexte} data-t-Texte gefunden — greift der Ausdruck noch?`);

// (c) Attribute: data-t-title / data-t-aria
let attribute = 0;
for (const m of seite.matchAll(/<[a-z0-9]+\b[^>]*?\sdata-t-(title|aria)\b[^>]*>/g)) {
  const attr = m[1] === "title" ? "title" : "aria-label";
  for (const art of m[0].matchAll(/\sdata-t-(title|aria)\b/g)) {
    const name = art[1] === "title" ? "title" : "aria-label";
    const wert = m[0].match(new RegExp("\\s" + name + '="([^"]*)"'));
    assert.ok(wert, `data-t-${art[1]} ohne ${name}-Attribut: ${m[0].slice(0, 80)}`);
    merke(entkomme(wert[1]), "data-t-" + art[1]);
    attribute++;
  }
  void attr;
}
assert.ok(attribute >= 5, `nur ${attribute} übersetzte Attribute gefunden — greift der Ausdruck noch?`);

// (d) Dynamische Aufrufe: t(<Ausdruck>). Die Liste ist bewusst geschlossen —
// ein neuer dynamischer Aufruf muss hier mit seiner Schlüsselquelle eingetragen
// werden, sonst prüft niemand, ob seine Texte im Wörterbuch stehen.
const DYNAMISCH = {
  "el.dataset.t": [], "el.dataset.tTitle": [], "el.dataset.tAria": [], // oben über data-t erfasst
  "u.label": ["1. Aufruf", "2. Aufruf", "Letzter Aufruf"],
  "lage.wort": ["Spielpunkt", "Satzpunkt"],
  "lage.status": [
    "✓ Ergebnis steht in BTP fest — Änderung nur über die Turnierleitung.",
    "✓ In BTP steht für dieses Spiel schon ein Ergebnis — Änderung nur über die Turnierleitung.",
    "✓ Übermittelt — die Turnierleitung kann übernehmen.",
    "Ergebnis wird übermittelt … wird automatisch wiederholt, bis es ankommt.",
    "Bitte zuerst den Sieger wählen.",
  ],
  // Der Grund kommt vom Turnier-PC und bleibt, wie er ist — übersetzt wird
  // nur der eigene Ersatztext der Seite.
  "STATE.resultRejected": ["Der Turnier-PC hat das Ergebnis abgelehnt."],
  "STATE.match.demoKlasse": [], // = DEMO_SZENARIEN.klasse, unten erfasst
  "ANORDNUNG_LABEL[anordnungWahl]": ["automatisch", "nebeneinander (links–rechts)", "übereinander (vorn–hinten)"],
  "sz.titel": [], "sz.text": [], "sz.klasse": [], // unten aus DEMO_SZENARIEN gelesen
};
{
  const gefunden = new Set();
  for (const m of ohneWoerterbuch.matchAll(/(?<![\w.$])t\((?!['")])([^,)]+)[,)]/g)) {
    if (m[1].trim() === "de") continue; // die Definition `function t(de, werte)`
    gefunden.add(m[1].trim());
  }
  assert.deepEqual([...gefunden].sort(), Object.keys(DYNAMISCH).sort(),
    "dynamische t()-Aufrufe: Liste DYNAMISCH in diesem Test nachziehen");
  for (const [ausdruck, texte] of Object.entries(DYNAMISCH)) {
    for (const text of texte) {
      assert.ok(ohneWoerterbuch.includes("'" + text + "'"),
        `Quelle von t(${ausdruck}): „${text}" steht nicht mehr wörtlich in der Seite`);
      merke(text, "t(" + ausdruck + ")");
    }
  }
}
{
  const start = seite.indexOf("const DEMO_SZENARIEN = [");
  const block = seite.slice(start, seite.indexOf("\n];", start));
  let n = 0;
  for (const m of block.matchAll(/\b(titel|text|klasse): '((?:[^'\\]|\\.)*)'/g)) { merke(literal(m[2]), "DEMO_SZENARIEN." + m[1]); n++; }
  assert.equal(n, 18, "sechs Übungsspiele mit Titel, Text und Klasse");
}

// ── 1. + 2. Vollständigkeit, Waisen, Platzhalter ─────────────────────────
const platzhalter = (s) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
const fehlend = [...schluessel].filter(([text]) => !Object.hasOwn(EN, text));
assert.deepEqual(fehlend.map(([text, woher]) => `${woher}: ${text}`), [], "Texte ohne englische Fassung");
const waisen = Object.keys(EN).filter((k) => !schluessel.has(k));
assert.deepEqual(waisen, [], "Wörterbuch-Einträge, die die Seite nicht (mehr) benutzt");
for (const [de, en] of Object.entries(EN)) {
  assert.equal(typeof en, "string");
  assert.ok(en.trim(), `leere Übersetzung für „${de}"`);
  assert.deepEqual(platzhalter(en), platzhalter(de), `Platzhalter weichen ab bei „${de}"`);
}

// Schlupflöcher der Erfassung oben, je eines:
// (a) `t("…")` mit doppelten Anführungszeichen sähe weder die Literal- noch
//     die Ausdrucks-Suche — die Seite schreibt Texte einheitlich mit '…'.
assert.ok(!/(?<![\w.$])t\("/.test(ohneWoerterbuch), 't("…") gefunden: Texte für t() stehen in einfachen Anführungszeichen');
// (b) Jeder Platzhalter eines Übungsspiel-Texts muss aus `demoTextWerte`
//     kommen, sonst stünde „{m6}" wörtlich auf dem Bildschirm.
{
  const werte = new Function(funktion("demoTextWerte") + "\nreturn demoTextWerte;")()({ ziel: 21, cap: 30, pause: 11 });
  for (const [text, woher] of schluessel) {
    if (!String(woher).startsWith("DEMO_SZENARIEN.text")) continue;
    for (const p of platzhalter(text)) assert.ok(p in werte, `Übungsspiel-Text „${text}": demoTextWerte liefert {${p}} nicht`);
  }
}
// (c) Das Wort im Verlauf-Graphen wird nachträglich ersetzt; ändert sich der
//     SVG-Text, bliebe der Graph still deutsch.
assert.ok(funktion("timelineSetSvg").includes(" Ballwechsel</text>"), "timelineSetSvg schreibt nicht mehr „N Ballwechsel“ — Ersetzung beim Verlauf nachziehen");
assert.ok(ohneWoerterbuch.includes(".replace(' Ballwechsel<', ' ' + t('Ballwechsel') + '<')"));

// ── 3. Keine Mischtexte mehr ─────────────────────────────────────────────
for (const rest of ["fillBilingual", "megaphoneSubtitle", "megaphone-subtitle", "t-en", "s-en",
  "Confirm presence", "Back to interval", "· Walkover", "Scoreboard:", "· Umpire", "Service judge",
  "Call official", "Cancel call", "Finish normally", "· Continue", "· Apply", "· Back", "who wins?"]) {
  assert.ok(!ohneWoerterbuch.includes(rest), `Rest der Zweisprachigkeit außerhalb des Wörterbuchs: „${rest}"`);
}
// Die eine gewollte Ausnahme: Die Überschrift des Umschalters muss in beiden
// Sprachen zu finden sein — sonst kommt niemand aus der fremden zurück.
assert.match(seite, /<div class="settings-section">Sprache · Language<\/div>/);
assert.match(seite, /id="sprache-de">Deutsch</);
assert.match(seite, /id="sprache-en">English</);

// ── 4. Verhalten ─────────────────────────────────────────────────────────
function seiteMit(sprache, namen, attrappen = {}) {
  const alle = { SPRACHE: sprache, EN, ...attrappen };
  return new Function(...Object.keys(alle),
    namen.map(funktion).join("\n") + "\nreturn { " + namen.join(", ") + " };")(...Object.values(alle));
}
{
  const de = seiteMit("de", ["t"]);
  const en = seiteMit("en", ["t"]);
  assert.equal(de.t("Weiterspielen"), "Weiterspielen");
  assert.equal(en.t("Weiterspielen"), EN["Weiterspielen"]);
  assert.equal(de.t("Schritt {n} von {m}", { n: 2, m: 3 }), "Schritt 2 von 3");
  assert.equal(en.t("Schritt {n} von {m}", { n: 2, m: 3 }), "Step 2 of 3");
  assert.equal(en.t("Unbekannter Text {x}", { x: 1 }), "Unbekannter Text 1", "ohne Eintrag bleibt der deutsche Text");
  assert.equal(en.t("{a} und {b}", { a: 1 }), "1 und {b}", "ein fehlender Wert lässt den Platzhalter sichtbar stehen");
  assert.equal(en.t("constructor"), "constructor", "geerbte Eigenschaften sind keine Übersetzungen");
  assert.equal(en.t("Satz {n}", { n: 0 }), "Game 0", "0 ist ein Wert");
}
{
  // Geparkte Ergebnisse: Im Deutschen wortgleich die Inline-Kopie (die
  // test-inline-kopien.mjs gegen das Modul hält), im Englischen eigene Sätze.
  const namen = ["t", "saetzeText", "spielName", "bandText", "frageText", "verwerfenText",
    "geparktWerte", "geparktBand", "geparktFrageText", "geparktVerwerfenText"];
  const e = { matchNr: 12, auftrag: { matchId: 7, sets: [{ a: 21, b: 10 }, { a: 21, b: 15 }] } };
  const ohneNr = { matchNr: null, auftrag: { matchId: 8, sets: [], walkover: true, winner: "a" } };
  const de = seiteMit("de", namen);
  assert.equal(de.geparktBand([e, ohneNr]), de.bandText([e, ohneNr]));
  assert.equal(de.geparktBand([]), "");
  assert.equal(de.geparktFrageText(e), de.frageText(e));
  assert.equal(de.geparktVerwerfenText(e), de.verwerfenText(e));
  const en = seiteMit("en", namen);
  assert.equal(en.geparktBand([]), "");
  assert.equal(en.geparktBand([e]), "Result of match no. 12 (21:10, 21:15) not delivered — tell the tournament desk");
  assert.match(en.geparktBand([e, ohneNr]), / \(\+1 more\)$/);
  assert.ok(!/Spiel|kampflos|Ergebnis|früheren/.test(en.geparktBand([ohneNr])),
    `englisches Band ohne deutsche Reste: ${en.geparktBand([ohneNr])}`);
  assert.ok(en.geparktFrageText(e).includes("21:10, 21:15"));
  assert.ok(en.geparktVerwerfenText(e).includes("match no. 12"));
}
{
  // Statuszeile des Abschlusses: `abschlussLage` (Inline-Kopie) bleibt deutsch.
  const namen = ["t", "abschlussLage", "abschlussStatus"];
  const mit = (sprache, STATE) => {
    const s = seiteMit(sprache, namen, { STATE });
    return s.abschlussStatus(s.abschlussLage(STATE));
  };
  for (const zustand of [{ finalized: true }, { ueberholt: true }, { submittedOk: true }, { pendingResult: {} }, { needWinner: true }]) {
    const de = mit("de", zustand);
    assert.ok(de, JSON.stringify(zustand));
    assert.equal(mit("en", zustand), EN[de], JSON.stringify(zustand));
  }
  assert.equal(mit("en", {}), "");
  assert.equal(mit("de", { resultRejected: "Match-ID passt nicht" }),
    "✗ Nicht angenommen: Match-ID passt nicht — bitte den Stand prüfen oder die Turnierleitung ansprechen.");
  const fremd = mit("en", { resultRejected: "Match-ID passt nicht" });
  assert.ok(fremd.includes("Match-ID passt nicht") && !fremd.includes("Nicht angenommen"),
    "der Grund des Turnier-PCs bleibt wörtlich, der Rahmen ist englisch");
  assert.ok(!/Turnier-PC/.test(mit("en", { resultRejected: "Der Turnier-PC hat das Ergebnis abgelehnt." })),
    "der eigene Ersatztext wird mit übersetzt");
}
{
  // Feste Blatt-Texte: hin und zurück ohne Neuladen. Der deutsche Text wird
  // am Element gemerkt — der Rückweg darf nicht aus dem englischen übersetzen.
  const el = (text) => ({ dataset: {}, textContent: text, title: "Mehr", attr: "Akkustand des Tablets",
    getAttribute() { return this.attr; }, setAttribute(_, v) { this.attr = v; } });
  const blatt = el("  Spiel wieder\n   öffnen ");
  const document = { querySelectorAll: () => [blatt] };
  const lauf = (sprache) => seiteMit(sprache, ["t", "statischUebersetzen"], { document }).statischUebersetzen();
  lauf("en");
  assert.equal(blatt.textContent, EN["Spiel wieder öffnen"], "Umbrüche im Quelltext stören den Schlüssel nicht");
  assert.equal(blatt.title, EN["Mehr"]);
  assert.equal(blatt.attr, EN["Akkustand des Tablets"]);
  lauf("en");
  assert.equal(blatt.textContent, EN["Spiel wieder öffnen"], "zweimal Englisch bleibt Englisch");
  lauf("de");
  assert.equal(blatt.textContent, "Spiel wieder öffnen");
  assert.equal(blatt.title, "Mehr");
  assert.equal(blatt.attr, "Akkustand des Tablets");
}

// Die Wahl hängt am Gerät, nicht am Feld — wie Anordnung und große Schrift.
assert.match(seite, /const SPRACHE_KEY = 'badhub\.tablet\.sprache';/);
// Kein Neuladen beim Umschalten: Ein laufendes Spiel darf nicht unterbrochen werden.
assert.ok(!/location\.reload|neuLaden\(/.test(funktion("setzeSprache")), "Sprachwechsel lädt die Seite nicht neu");

console.log(`✓ Tablet-Sprache: ${schluessel.size} Texte, alle mit englischer Fassung; Verhalten in beiden Sprachen geprüft`);
