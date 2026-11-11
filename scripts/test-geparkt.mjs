/**
 * Geparkte Ergebnisse am Zähl-Tablet (Spec tablet-stand-robust, Regel C;
 * AK18/AK19; ADR 0085).
 *
 * Ein Ergebnis, das der Turnier-PC nicht angenommen hat, darf weder verloren
 * gehen noch das Folgespiel blockieren. Teil 1 prüft das Modul
 * `src/io/geparkt.mjs`, Teil 2 den ECHTEN Code der Seite: Die Funktionen
 * werden aus `tablet.html` geschnitten und mit einem falschen Speicher,
 * einer falschen Uhr und einem falschen Turnier-PC ausgeführt.
 */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import * as g from "../src/io/geparkt.mjs";

const hier = dirname(fileURLToPath(import.meta.url));
const seite = readFileSync(join(hier, "..", "src-tauri", "assets", "tablet.html"), "utf8");

// ══ Teil 1: das Modul ═════════════════════════════════════════════════
const A7 = { matchId: 7, courtId: 101, courtLabel: "Feld 1", sets: [{ a: 21, b: 10 }, { a: 21, b: 15 }] };
const A8 = { matchId: 8, courtId: 101, courtLabel: "Feld 1", sets: [{ a: 21, b: 19 }, { a: 5, b: 2 }], retired: true, winner: 1 };
const A9 = { matchId: 9, courtId: 101, courtLabel: "Feld 1", sets: [], walkover: true, winner: 2 };

// Wann wird geparkt?
assert.equal(g.mussParken(A7, 8), true, "anderes Spiel kommt");
assert.equal(g.mussParken(A7, null), true, "Feld wird leer");
assert.equal(g.mussParken(A7, 7), false, "dasselbe Spiel: der Auftrag bleibt, wo er ist");
assert.equal(g.mussParken(null, 8), false, "kein Auftrag");

// Parken, Auflösen, Finden.
let liste = g.parken([], A7, 12, 1000);
assert.deepEqual(liste, [{ auftrag: A7, matchNr: 12, seitMs: 1000 }]);
liste = g.parken(liste, A8, undefined, 2000);
assert.equal(liste.length, 2);
assert.equal(liste[1].matchNr, null, "ohne Nummer: null");
const neuer7 = { ...A7, sets: [{ a: 21, b: 0 }] };
const ersetzt = g.parken(liste, neuer7, 12, 3000);
assert.deepEqual(ersetzt.map((e) => e.auftrag.matchId), [8, 7], "selbes Spiel: ersetzt, nicht verdoppelt");
assert.equal(liste.length, 2, "die Eingabe bleibt unberührt");
assert.equal(g.geparktFuer(liste, 7).auftrag, A7);
assert.equal(g.geparktFuer(liste, 99), null);
assert.equal(g.geparktFuer(liste, null), null);
assert.deepEqual(g.aufloesen(liste, 7).map((e) => e.auftrag.matchId), [8]);
assert.deepEqual(g.aufloesen(liste, 99), liste);

// Deckel: die ältesten fallen heraus.
let viele = [];
for (let i = 1; i <= g.GEPARKT_MAX + 2; i++) viele = g.parken(viele, { matchId: i, sets: [] }, i, i);
assert.equal(viele.length, g.GEPARKT_MAX);
assert.equal(viele[0].auftrag.matchId, 3);

// Was wird im Hintergrund zugestellt? Alles außer dem Spiel auf dem Feld.
assert.deepEqual(g.zuPruefen(liste, 7).map((e) => e.auftrag.matchId), [8]);
assert.deepEqual(g.zuPruefen(liste, null).map((e) => e.auftrag.matchId), [7, 8]);
assert.deepEqual(g.zuPruefen(liste, 99).map((e) => e.auftrag.matchId), [7, 8]);
// „Jetzt senden" gibt den Eintrag frei — dann geht er auch raus, wenn sein
// Spiel auf dem Feld liegt. Der Auftrag bleibt dasselbe Objekt.
const frei = g.freigeben(liste, 7);
assert.deepEqual(g.zuPruefen(frei, 7).map((e) => e.auftrag.matchId), [7, 8]);
assert.equal(g.geparktFuer(frei, 7).auftrag, A7);
assert.equal(g.geparktFuer(frei, 7).freigegeben, true);
assert.equal(g.geparktFuer(liste, 7).freigegeben, undefined, "die Eingabe bleibt unberührt");
assert.equal(g.geparktFuer(g.parken(frei, A7, 12, 9), 7).freigegeben, undefined, "neu geparkt = neu fragen");
assert.equal(g.naechstePruefungMs(false), 15000);
assert.equal(g.naechstePruefungMs(true), 60000);

// Karenz: frisch Geparktes ist noch nicht zu sehen.
assert.deepEqual(g.geparktSichtbar(liste, 1000 + g.GEPARKT_KARENZ_MS - 1), []);
assert.deepEqual(g.geparktSichtbar(liste, 1000 + g.GEPARKT_KARENZ_MS).map((e) => e.auftrag.matchId), [7]);
assert.equal(g.geparktSichtbar([{ auftrag: A7 }], 0).length, 1, "ohne Zeitstempel: sichtbar");

// Texte.
assert.equal(g.saetzeText(A7), "21:10, 21:15");
assert.equal(g.saetzeText(A8), "21:19, 5:2, Aufgabe");
assert.equal(g.saetzeText(A9), "kampflos");
assert.equal(g.saetzeText({ sets: [] }), "ohne Sätze");
assert.equal(g.saetzeText({ sets: [null, { a: "x", b: 1 }, { a: 21, b: 3 }] }), "21:3");
assert.equal(g.bandText([]), "");
assert.equal(g.bandText([liste[0]]), "Ergebnis von Spiel Nr. 12 (21:10, 21:15) nicht zugestellt — Turnierleitung informieren");
assert.equal(g.bandText(liste), "Ergebnis von Spiel Nr. 12 (21:10, 21:15) nicht zugestellt — Turnierleitung informieren (+1 weitere)");
assert.equal(g.spielName(liste[1]), "einem früheren Spiel");
assert.equal(g.frageText(liste[0]), "Für dieses Spiel liegt ein nicht zugestelltes Ergebnis vor (21:10, 21:15).");
assert.equal(g.verwerfenText(liste[0]), "Ergebnis von Spiel Nr. 12 (21:10, 21:15) wirklich verwerfen? Es steht dann nicht in BTP.");

// Laden aus dem Speicher: Unlesbares wirft nicht.
assert.deepEqual(g.geparktLaden(null), []);
assert.deepEqual(g.geparktLaden("kaputt{"), []);
assert.deepEqual(g.geparktLaden('{"a":1}'), []);
assert.deepEqual(g.geparktLaden(JSON.stringify(liste)), liste);
const schmutz = [null, 3, {}, { auftrag: null }, { auftrag: { matchId: "7", sets: [] } }, { auftrag: { matchId: 7 } },
  { auftrag: [] }, liste[0], { auftrag: { matchId: 7, sets: [] }, matchNr: 12, seitMs: 5 }];
assert.deepEqual(g.geparktLaden(JSON.stringify(schmutz)), [{ auftrag: { matchId: 7, sets: [] }, matchNr: 12, seitMs: 5 }],
  "nur gültige Einträge, je Spiel der jüngste");
for (const e of schmutz.slice(0, 7)) assert.equal(g.geparktGueltig(e), false);
assert.equal(g.geparktGueltig({ auftrag: { matchId: 7, sets: Array(g.GEPARKT_MAX_SAETZE).fill({ a: 1, b: 0 }) } }), true);
assert.equal(g.geparktGueltig({ auftrag: { matchId: 7, sets: Array(g.GEPARKT_MAX_SAETZE + 1).fill({ a: 1, b: 0 }) } }), false,
  "aufgeblähter Eintrag wird nicht geladen");

// ══ Teil 2: der echte Code der Seite ══════════════════════════════════
function funktion(name) {
  const start = seite.indexOf("function " + name + "(");
  assert.ok(start >= 0, `tablet.html: function ${name} fehlt`);
  const vorsatz = seite.slice(Math.max(0, start - 6), start) === "async " ? "async " : "";
  let tiefe = 0;
  for (let i = seite.indexOf("{", start); i < seite.length; i++) {
    if (seite[i] === "{") tiefe++;
    else if (seite[i] === "}" && --tiefe === 0) return vorsatz + seite.slice(start, i + 1);
  }
  throw new Error(`tablet.html: function ${name} endet nicht`);
}

const FUNKTIONEN = [
  "geparktGueltig", "geparktLaden", "mussParken", "parken", "aufloesen", "geparktFuer", "zuPruefen",
  "freigeben", "geparktSichtbar", "naechstePruefungMs", "saetzeText", "spielName", "bandText", "frageText", "verwerfenText",
  "geparktLesen", "geparktSichern", "parkeFremdenAuftrag", "geparktEntfernen", "zeigeGeparktMeldung",
  "geparktAntwort", "planeGeparktPruefung", "sendeGeparktes", "pruefeGeparkte",
  "geparktBeiFinalisierungAufloesen", "geparktRueckkehr", "geparktJetztSenden", "geparktVerwerfen",
  "naechsterSchritt", "sendeauftragErledigt", "sendeauftragBeiFinalisierungLoslassen",
  "beiMatchAssigned", "loadMatch", "clearMatch", "submit", "trySubmitPending",
];

/** Ein Tablet an Feld 101. `speicher` überlebt mehrere „Seitenaufrufe". */
function tablet(speicher = {}) {
  const uhr = { t: 1_000_000 };
  const timer = [];
  const log = [];
  const anfragen = [];
  let antwort = () => ({ ok: true, json: async () => ({ ok: true }) });
  const STATE = {
    match: null, pendingResult: null, submittedOk: false, resultRejected: null, ueberholt: false,
    finalized: false, finished: false, locked: false, occupied: false, teamOnSide: null,
    setsCompleted: [], retired: false, walkover: false, cascadeWalkover: false, retiredWinner: null,
    events: [], rallyLog: null, history: [], cards: [],
  };
  const localStorage = {
    getItem: (k) => (k in speicher ? speicher[k] : null),
    setItem: (k, v) => { speicher[k] = String(v); },
    removeItem: (k) => { delete speicher[k]; },
  };
  const attrappen = {
    STATE, localStorage,
    COURT_ID: 101, COURT_LABEL: "Feld 1", BASE_PATH: "", RESULT_ABORT_MS: 1000, RESULT_RETRY_MS: 5000,
    RESULT_OHNE_GRUND: "abgelehnt", PERSIST_KEY: "badhub.tablet.state.101",
    tlog: (stufe, was, d) => log.push([was, d]),
    render: () => {}, persistState: () => { speicher["badhub.tablet.state.101"] = JSON.stringify({ match: STATE.match, pendingResult: STATE.pendingResult }); },
    clearPersistedState: () => { delete speicher["badhub.tablet.state.101"]; },
    sendAlert: () => {}, sendScoreUpdate: () => {}, sendRallySync: () => {}, sendMatchEventSync: () => {},
    uploadTabletLog: () => {}, clearReidentify: () => {}, syncServerClock: () => {}, clearInterval: () => {},
    Date: { now: () => uhr.t },
    setTimeout: (f, ms) => { const id = timer.length + 1; timer.push({ id, f, ms, aktiv: true }); return id; },
    clearTimeout: (id) => { const x = timer.find((y) => y.id === id); if (x) x.aktiv = false; },
    AbortController,
    fetch: async (url, opt) => { const body = JSON.parse(opt.body); anfragen.push(body); return antwort(body); },
  };
  const namen = Object.keys(attrappen);
  const quelle = (name) => {
    const m = seite.match(new RegExp("const " + name + " = (\\d+);"));
    assert.ok(m, `tablet.html: Konstante ${name} fehlt`);
    return `const ${name} = ${m[1]};`;
  };
  const f = new Function(
    ...namen,
    ["GEPARKT_MAX", "GEPARKT_MAX_SAETZE", "GEPARKT_ERSTE_PRUEFUNG_MS", "GEPARKT_TAKT_MS", "GEPARKT_KARENZ_MS"].map(quelle).join("\n") +
      "\nconst GEPARKT_KEY = `badhub.tablet.geparkt.${COURT_ID}`;\n" +
      "let resultRetryTimer = null, breakTimer = null, cardsMatchId = null, healthNachfragenRest = 0;\n" +
      "let geparktTimer = null, geparktGeprueft = false, geparktLaeuft = false, geparktZuletztMs = 0;\n" +
      "let geparktMeldung = '', geparktMeldungTimer = null, geparktVerwerfenFrage = null, geparktKarenzTimer = null;\n" +
      FUNKTIONEN.map(funktion).join("\n") +
      "\nlet GEPARKT = geparktLesen();\n" +
      "return { geparkt: () => GEPARKT, meldung: () => geparktMeldung, band: () => geparktMeldung || bandText(geparktSichtbar(GEPARKT, Date.now())),\n" +
      "  planeGeparktPruefung, pruefeGeparkte, geparktRueckkehr, geparktJetztSenden, geparktVerwerfen,\n" +
      "  beiMatchAssigned, loadMatch, clearMatch, submit, trySubmitPending };",
  )(...namen.map((n) => attrappen[n]));
  /** Feuert alle aktiven Timer mit genau dieser Frist. */
  const feuere = async (ms) => {
    const faellig = timer.filter((x) => x.aktiv && x.ms === ms);
    for (const x of faellig) { x.aktiv = false; await x.f(); }
    return faellig.length;
  };
  const geplant = () => timer.filter((x) => x.aktiv).map((x) => x.ms).sort((a, b) => a - b);
  return {
    STATE, speicher, uhr, log, anfragen, feuere, geplant, ...f,
    setzeAntwort: (fn) => { antwort = fn; },
    /** Spiel beenden, wie es der Bediener täte. */
    spielFertig: () => { STATE.finished = true; STATE.teamOnSide = { a: "left", b: "right" }; STATE.setsCompleted = [{ left: 21, right: 10 }, { left: 21, right: 15 }]; },
  };
}
const warte = (ms = 15) => new Promise((r) => setTimeout(r, ms));
const absage = (extra = {}) => () => ({ ok: true, json: async () => ({ ok: false, error: "Kein Match auf diesem Court.", ...extra }) });
const KEY = "badhub.tablet.geparkt.101";

// ── AK19: Spielwechsel mit offenem Auftrag → geparkt, Folgespiel sendbar ──
{
  const t = tablet();
  t.setzeAntwort(absage());
  t.loadMatch({ matchId: 7, matchNr: 12 });
  t.spielFertig();
  t.submit();
  await warte();
  assert.equal(t.STATE.pendingResult.matchId, 7, "Auftrag offen, wird wiederholt");

  t.loadMatch({ matchId: 8, matchNr: 13 });
  assert.equal(t.STATE.pendingResult, null, "der Auftrag blockiert das neue Spiel nicht");
  assert.deepEqual(t.geparkt().map((e) => [e.auftrag.matchId, e.matchNr]), [[7, 12]], "geparkt samt Spielnummer");
  assert.ok(t.speicher[KEY], "eigener Speicher-Schlüssel");
  assert.ok(!JSON.stringify(JSON.parse(t.speicher["badhub.tablet.state.101"])).includes('"matchId":7'), "nicht im Spielstand");
  assert.ok(t.log.some(([was, d]) => was === "ergebnis_geparkt" && Object.keys(d).sort().join() === "anzahl,matchId"), "Log nur mit Kennzahlen");
  assert.ok(!t.geplant().includes(5000), "der alte Wiederholungs-Timer ist gestoppt");
  assert.ok(t.geplant().includes(15000), "erste Hintergrund-Prüfung nach 15 s");

  // Karenz: erst nach 4 s steht das Band.
  assert.equal(t.band(), "");
  t.uhr.t += 4000;
  assert.equal(t.band(), "Ergebnis von Spiel Nr. 12 (21:10, 21:15) nicht zugestellt — Turnierleitung informieren");

  // Das Folgespiel sendet sein eigenes Ergebnis.
  t.spielFertig();
  t.setzeAntwort((body) => ({ ok: true, json: async () => (body.matchId === 8 ? { ok: true } : { ok: false, error: "x" }) }));
  t.submit();
  await warte();
  assert.equal(t.anfragen.at(-1).matchId, 8);
  assert.equal(t.STATE.submittedOk, true, "Folgespiel angenommen");
  assert.equal(t.geparkt().length, 1, "das alte bleibt geparkt und sichtbar");

  // Hintergrund: wiederholbare Absage → bleibt, nächster Takt 60 s.
  t.setzeAntwort(absage());
  assert.equal(await t.feuere(15000), 1);
  assert.equal(t.anfragen.at(-1).matchId, 7, "die Prüfung schickt das geparkte Ergebnis");
  assert.equal(t.geparkt().length, 1);
  assert.ok(t.geplant().includes(60000), "danach alle 60 s");

  // AK18: „überholt" löst den Eintrag auf, mit kurzer Entwarnung.
  t.setzeAntwort(absage({ superseded: true }));
  await t.feuere(60000);
  assert.equal(t.geparkt().length, 0);
  assert.ok(!(KEY in t.speicher), "Schlüssel geräumt");
  assert.equal(t.band(), "Ergebnis von Spiel Nr. 12 nicht mehr nötig — in BTP steht schon eines.");
  assert.ok(!t.geplant().includes(60000) && !t.geplant().includes(15000), "nichts mehr zu prüfen");
  await t.feuere(8000);
  assert.equal(t.band(), "", "die Entwarnung verschwindet");
}

// ── AK19: Feld geleert → geparkt, überlebt `match_cleared` und Reload ──
{
  const speicher = {};
  const t = tablet(speicher);
  t.setzeAntwort(absage());
  t.loadMatch({ matchId: 7, matchNr: 12 });
  t.spielFertig();
  t.submit();
  await warte();
  t.clearMatch();
  assert.equal(t.STATE.match, null);
  assert.equal(t.STATE.pendingResult, null);
  assert.ok(!("badhub.tablet.state.101" in speicher), "der Spielstand ist geleert");
  assert.equal(t.geparkt().length, 1, "das Ergebnis nicht");

  // Reload: neue Seite, derselbe Speicher.
  const t2 = tablet(speicher);
  assert.deepEqual(t2.geparkt().map((e) => e.auftrag.matchId), [7]);
  t2.uhr.t += 60_000;
  assert.match(t2.band(), /Spiel Nr\. 12 \(21:10, 21:15\) nicht zugestellt/);
  t2.planeGeparktPruefung();
  assert.ok(t2.geplant().includes(15000), "nach dem Reload läuft die Prüfung wieder an");

  // Zustellung gelingt doch noch (`ok`) → aufgelöst.
  t2.setzeAntwort(() => ({ ok: true, json: async () => ({ ok: true }) }));
  await t2.feuere(15000);
  assert.equal(t2.geparkt().length, 0);
  assert.equal(t2.band(), "Ergebnis von Spiel Nr. 12 ist zugestellt.");
}

// ── AK19: Das geparkte Spiel kommt zurück → Abfrage, kein stilles Senden ──
{
  const t = tablet();
  t.setzeAntwort(absage());
  t.loadMatch({ matchId: 7, matchNr: 12 });
  t.spielFertig();
  t.submit();
  await warte();
  t.clearMatch();
  const vorher = t.anfragen.length;

  t.loadMatch({ matchId: 7, matchNr: 12 });
  assert.equal(t.geparktRueckkehr().auftrag.matchId, 7, "die Abfrage steht an");
  assert.equal(t.STATE.pendingResult, null, "nichts wird von selbst gesendet");
  await t.pruefeGeparkte();
  assert.equal(t.anfragen.length, vorher, "auch die Hintergrund-Prüfung schickt es nicht");

  // „Jetzt senden" — der Turnier-PC lehnt (noch) ab: Der Eintrag bleibt
  // geparkt und sichtbar, die Frage kommt nicht wieder, und das laufende
  // Spiel behält seinen eigenen Sendeauftrag frei.
  t.setzeAntwort(absage());
  t.geparktJetztSenden();
  await warte();
  assert.equal(t.anfragen.at(-1).matchId, 7);
  assert.deepEqual(t.anfragen.at(-1).sets, [{ a: 21, b: 10 }, { a: 21, b: 15 }], "genau das geparkte Ergebnis");
  assert.equal(t.geparkt().length, 1, "abgelehnt: bleibt geparkt, geht nicht verloren");
  assert.equal(t.geparktRueckkehr(), null, "die Frage ist beantwortet");
  assert.equal(t.STATE.pendingResult, null, "das neu gespielte Spiel kann sein eigenes Ergebnis senden");
  assert.ok(t.geplant().includes(60000), "wird weiter versucht");
  t.uhr.t += 5000;
  assert.match(t.band(), /nicht zugestellt/);

  // Nächster Versuch: angenommen.
  t.setzeAntwort(() => ({ ok: true, json: async () => ({ ok: true }) }));
  await t.feuere(60000);
  assert.equal(t.geparkt().length, 0);
  assert.equal(t.band(), "Ergebnis von Spiel Nr. 12 ist zugestellt.");
}
{
  // Rückkehr unbeantwortet, dann geht das Spiel wieder vom Feld: Die
  // Hintergrund-Prüfung muss von selbst wieder anlaufen.
  const t = tablet();
  t.setzeAntwort(absage());
  t.loadMatch({ matchId: 7, matchNr: 12 });
  t.spielFertig();
  t.submit();
  await warte();
  t.clearMatch();
  t.loadMatch({ matchId: 7, matchNr: 12 });
  assert.ok(!t.geplant().includes(15000) && !t.geplant().includes(60000), "solange gefragt wird: keine Prüfung");
  t.clearMatch();
  assert.ok(t.geplant().includes(15000), "Feld wieder leer: Prüfung läuft wieder an");
  t.loadMatch({ matchId: 9, matchNr: 14 });
  assert.ok(t.geplant().includes(15000), "anderes Spiel: ebenso");
}
{
  // Deckel: Der verdrängte Eintrag steht im Log.
  const t = tablet();
  t.setzeAntwort(absage());
  for (let i = 1; i <= 6; i++) {
    t.loadMatch({ matchId: i, matchNr: i });
    t.spielFertig();
    t.submit();
    await warte(5);
  }
  t.clearMatch();
  assert.equal(t.geparkt().length, 5);
  assert.ok(t.log.some(([was, d]) => was === "geparkt_deckel" && JSON.stringify(d) === '{"matchIds":[1]}'), "verdrängt wird nie still");
}
{
  // „Verwerfen" in der Abfrage.
  const t = tablet();
  t.setzeAntwort(absage());
  t.loadMatch({ matchId: 7, matchNr: 12 });
  t.spielFertig();
  t.submit();
  await warte();
  t.clearMatch();
  t.loadMatch({ matchId: 7, matchNr: 12 });
  const vorher = t.anfragen.length;
  t.geparktVerwerfen(7);
  assert.equal(t.geparkt().length, 0);
  assert.ok(!(KEY in t.speicher));
  assert.equal(t.anfragen.length, vorher, "verwerfen sendet nichts");
  assert.ok(t.log.some(([was, d]) => was === "geparkt_aufgeloest" && d.grund === "verworfen"));
}
{
  // Kommt das Spiel schon FEST zurück (Hand-Ergebnis), ist der Eintrag erledigt.
  const t = tablet();
  t.setzeAntwort(absage());
  t.loadMatch({ matchId: 7, matchNr: 12 });
  t.spielFertig();
  t.submit();
  await warte();
  t.clearMatch();
  t.beiMatchAssigned({ match: { matchId: 7, matchNr: 12, finalized: true } });
  assert.equal(t.geparkt().length, 0);
  assert.equal(t.geparktRueckkehr(), null, "keine Abfrage für ein festes Spiel");
}

// ── Normalfall: `match_cleared` überholt nur die Antwort — kein Fehlalarm ──
{
  const t = tablet();
  let freigabe;
  t.setzeAntwort(() => new Promise((r) => { freigabe = () => r({ ok: true, json: async () => ({ ok: true }) }); }));
  t.loadMatch({ matchId: 7, matchNr: 12 });
  t.spielFertig();
  t.submit();
  await warte(5);
  t.clearMatch(); // der WebSocket war schneller als die HTTP-Antwort
  assert.equal(t.geparkt().length, 1);
  assert.equal(t.band(), "", "in der Karenz: kein Band");
  freigabe();
  await warte();
  assert.equal(t.geparkt().length, 0, "die späte Annahme löst den Eintrag auf");
  assert.equal(t.band(), "", "und zwar still — es war nie etwas zu sehen");
  assert.ok(!t.geplant().includes(15000), "keine Prüfung mehr geplant");
}

// ── AK18 am laufenden Auftrag: „überholt" lässt los, ohne zu parken ──
{
  const t = tablet();
  t.setzeAntwort(absage({ superseded: true, error: "Für dieses Spiel steht in BTP schon ein Ergebnis." }));
  t.loadMatch({ matchId: 7, matchNr: 12 });
  t.spielFertig();
  t.submit();
  await warte();
  assert.equal(t.STATE.pendingResult, null, "losgelassen");
  assert.equal(t.STATE.ueberholt, true);
  assert.equal(t.STATE.submittedOk, false, "überholt ist keine Annahme");
  assert.equal(t.STATE.resultRejected, null, "und keine Ablehnung");
  assert.equal(t.geparkt().length, 0);
  assert.ok(!t.geplant().includes(5000), "kein weiterer Versuch");
  // Das nächste Spiel beginnt ohne den Vermerk.
  t.loadMatch({ matchId: 8, matchNr: 13 });
  assert.equal(t.STATE.ueberholt, false);
}

// ── Alter Host ohne „überholt": wiederholbare Absage → der Auftrag parkt ──
{
  const t = tablet();
  t.setzeAntwort(absage());
  t.loadMatch({ matchId: 7, matchNr: 12 });
  t.spielFertig();
  t.submit();
  await warte();
  assert.ok(t.STATE.pendingResult, "wird wiederholt wie bisher");
  t.loadMatch({ matchId: 8, matchNr: 13 });
  assert.equal(t.geparkt().length, 1, "sicherer Rückfall: geparkt statt blockiert");
}

// ── Die Liste reist nicht im Spiegel; der Versionsabgleich kennt sie ──
{
  const spiegel = funktion("standFuerSpiegel");
  assert.ok(!/GEPARKT|geparkt/.test(spiegel), "standFuerSpiegel trägt keine geparkten Ergebnisse");
  assert.match(seite, /if \(!STATE\.match && !STATE\.pendingResult && \(!GEPARKT\.length \|\| SPEICHER_GEHT\)\) \{\s*neuLaden\(msg\.marke\);/,
    "stiller Abgleich: kein Neuladen mit geparktem Ergebnis ohne Speicher");
  assert.match(seite, /if \(\(STATE\.match \|\| STATE\.pendingResult \|\| GEPARKT\.length\) && !SPEICHER_GEHT\) \{/,
    "Fernbefehl: vertagt mit geparktem Ergebnis ohne Speicher");
  assert.match(seite, /if \(GEPARKT\.length && Date\.now\(\) - geparktZuletztMs >= 10000\) \{\s*geparktZuletztMs = Date\.now\(\);\s*setTimeout\(pruefeGeparkte, 3000\);/,
    "Neuverbindung prüft — mit Mindestabstand und erst, wenn das Spiel auf dem Feld bekannt ist");
  // Verwerfen ist auf BEIDEN Wegen zweistufig.
  assert.match(seite, /const eintrag = geparktRueckkehr\(\);\s*if \(eintrag\) \{ geparktVerwerfenFrage = eintrag\.auftrag\.matchId; render\(\); \}/,
    "Rückkehr-Frage: „Verwerfen“ führt zur zweiten Abfrage");
  // Ein eigener offener Auftrag überlebt die Übernahme eines fremden Stands.
  assert.match(funktion("applyPersistedState"),
    /parkeFremdenAuftrag\(p\.match \? p\.match\.matchId : null\);\s*STATE\.pendingResult = p\.pendingResult \?\? STATE\.pendingResult \?\? null;/,
    "applyPersistedState parkt oder behält den lokalen Auftrag");
  assert.match(funktion("render"), /renderGeparkt\(\);/, "das Band hängt an jedem Render — also an jeder Ansicht");
}

console.log("✓ Geparkte Ergebnisse: alle Prüfungen bestanden");
