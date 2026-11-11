// Prüft die Inline-Kopien von src/io/monitorSeq.mjs und src/io/pushHealth.mjs
// in den Anzeige-Seiten gegen die Module (Spec monitor-stile-und-stream E2,
// ADR 0069; offener Punkt aus dem Security-Review #328).
//
// Die Kopien sind in ES5 umgeschrieben (andere Parameternamen, `var`), ein
// Wortvergleich ginge also nie auf. Stattdessen wird jede Kopie aus der Seite
// geschnitten, ausgeführt und auf denselben Eingaben gegen das Modul
// verglichen. Weicht eine Kopie im Verhalten ab, zeigt eine Anzeige Stände
// in falscher Ordnung oder hält einen toten Kanal für gesund — beides fällt
// in der Halle erst auf, wenn ein TV einfriert.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { anwenden } from "../src/io/monitorSeq.mjs";
import * as ph from "../src/io/pushHealth.mjs";
import * as lm from "../src/io/lobbyMeetingpoint.mjs";
import * as pf from "../src/io/pinFreigabe.mjs";
import * as us from "../src/io/undoStand.mjs";
import * as gp from "../src/io/geparkt.mjs";
import * as rr from "../src/io/resultRetry.mjs";
import * as aa from "../src/io/abschlussAnsicht.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

const SEITEN = ["monitor.html", "tafel.html", "overview.html", "stream.html"];
const hier = dirname(fileURLToPath(import.meta.url));

/** Schneidet `function name(...) { ... }` samt Rumpf aus dem Quelltext. */
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
function konstante(text, name) {
  const m = text.match(new RegExp("var " + name + " = (\\d+);"));
  return m ? Number(m[1]) : null;
}

const SEQ_FAELLE = [];
for (const g of [0, 5, 10, NaN]) for (const n of [0, 5, 6, 10, NaN]) for (const q of ["push", "fetch", "x"]) SEQ_FAELLE.push([g, n, q]);
const JETZT = 100000;
const ZUSTAENDE = [
  null, {}, { wsOpen: false }, { wsOpen: true, lastServerFrameMs: 0 },
  { wsOpen: true, lastServerFrameMs: JETZT - 1000 },
  { wsOpen: true, lastServerFrameMs: JETZT - 25000 },
  { wsOpen: true, lastServerFrameMs: JETZT - 1000, lastFetchOk: false },
  { wsOpen: true, lastServerFrameMs: JETZT - 1000, failures: 2 },
];

for (const seite of SEITEN) {
  const text = readFileSync(join(hier, "..", "src-tauri", "assets", seite), "utf8");
  const quellen = ["seqAnwenden", "pushGesund", "fallbackTakt", "kanalIstTot"].map((n) => funktion(text, n));
  ok(`${seite}: alle vier Kopien vorhanden`, quellen.every(Boolean), true);
  if (!quellen.every(Boolean)) continue;
  const konst = ["HERZSCHLAG_STILL_MS", "FALLBACK_LANGSAM_MS", "FALLBACK_SCHNELL_MS"];
  ok(`${seite}: Konstanten wie im Modul`, konst.map((k) => konstante(text, k)), konst.map((k) => ph[k]));
  const kopf = konst.map((k) => `var ${k} = ${konstante(text, k)};`).join("\n");
  // eslint-disable-next-line no-new-func
  const f = new Function(kopf + "\n" + quellen.join("\n") +
    "\nreturn { seqAnwenden, pushGesund, fallbackTakt, kanalIstTot };")();
  ok(`${seite}: seqAnwenden wie anwenden`,
    SEQ_FAELLE.map(([g, n, q]) => f.seqAnwenden(g, n, q)),
    SEQ_FAELLE.map(([g, n, q]) => anwenden(g, n, q)));
  ok(`${seite}: pushGesund wie Modul`,
    ZUSTAENDE.map((z) => f.pushGesund(z, JETZT)),
    ZUSTAENDE.map((z) => ph.pushGesund(z, JETZT)));
  ok(`${seite}: fallbackTakt wie Modul`,
    [[true, true], [true, false], [false, true], [false, false]].map(([a, b]) => f.fallbackTakt(a, b)),
    [[true, true], [true, false], [false, true], [false, false]].map(([a, b]) => ph.fallbackTakt(a, b)));
  const tot = [[false, 1, JETZT], [true, 0, JETZT], [true, JETZT - 1000, JETZT], [true, JETZT - 25000, JETZT]];
  ok(`${seite}: kanalIstTot wie Modul`,
    tot.map(([a, b, c]) => f.kanalIstTot(a, b, c)),
    tot.map(([a, b, c]) => ph.kanalIstTot(a, b, c)));
}

// ── Meetingpoint-Eintrag der Feldwahl (Spec meetingpoint-am-kiosk-tablet) ──
// Eine abweichende Kopie ließe den Knopf „Zurück zum Meetingpoint" auf einem
// Zähl-Tablet stehen oder würde die gemerkte Halle verlieren.
function seite(name) {
  return readFileSync(join(hier, "..", "src-tauri", "assets", name), "utf8");
}
function zeichenkette(text, name) {
  const m = text.match(new RegExp("(?:var|const) " + name + " = (?:\"([^\"]*)\"|'([^']*)');"));
  return m ? (m[1] ?? m[2]) : null;
}
const MERKER = [
  null, "", "{x", "null", "42", '["meetingpoint"]',
  JSON.stringify({ layout: "meetingpoint", court: null, halle: "Halle 2" }),
  JSON.stringify({ layout: "meetingpoint", court: 7 }),
  JSON.stringify({ layout: "meetingpoint" }),
  JSON.stringify({ layout: "tafel", court: 3, halle: "A" }),
];
{
  const text = seite("lobby.html");
  const namen = ["meetingpointZiel", "mpAusStatus", "merkerLesen", "warZuletztMeetingpoint", "ohneMeetingpointMarke"];
  const quellen = namen.map((n) => funktion(text, n));
  ok("lobby.html: Meetingpoint-Kopien vorhanden", quellen.every(Boolean), true);
  ok("lobby.html: Merker-Schlüssel wie im Modul", zeichenkette(text, "ZULETZT_KEY"), lm.ZULETZT_KEY);
  if (quellen.every(Boolean)) {
    // eslint-disable-next-line no-new-func
    const f = new Function(quellen.join("\n") + "\nreturn { " + namen.join(", ") + " };")();
    ok("lobby.html: meetingpointZiel wie Modul", f.meetingpointZiel(), lm.meetingpointZiel());
    const st = [[200, false], [404, true], [0, true], [0, false], [500, true], [302, false], [undefined, undefined]];
    ok("lobby.html: mpAusStatus wie Modul", st.map(([a, b]) => f.mpAusStatus(a, b)), st.map(([a, b]) => lm.mpAusStatus(a, b)));
    ok("lobby.html: warZuletztMeetingpoint wie Modul", MERKER.map(f.warZuletztMeetingpoint), MERKER.map(lm.warZuletztMeetingpoint));
    ok("lobby.html: ohneMeetingpointMarke wie Modul", MERKER.map(f.ohneMeetingpointMarke), MERKER.map(lm.ohneMeetingpointMarke));
  }
}
{
  const text = seite("anzeige.html");
  const quellen = ["merkerLesen", "ohneMeetingpointMarke"].map((n) => funktion(text, n));
  ok("anzeige.html: Marken-Kopie vorhanden", quellen.every(Boolean), true);
  ok("anzeige.html: Merker-Schlüssel wie im Modul", zeichenkette(text, "ZULETZT_KEY"), lm.ZULETZT_KEY);
  if (quellen.every(Boolean)) {
    // eslint-disable-next-line no-new-func
    const f = new Function(quellen.join("\n") + "\nreturn { ohneMeetingpointMarke };")();
    ok("anzeige.html: ohneMeetingpointMarke wie Modul", MERKER.map(f.ohneMeetingpointMarke), MERKER.map(lm.ohneMeetingpointMarke));
  }
}

// ── PIN-Freigabe (src/io/pinFreigabe.mjs): drei Seiten teilen den Schlüssel ──
const T0 = 1_000_000_000_000;
const FREIGABE_FAELLE = [
  [T0 + 1000, T0], [T0, T0], [T0 - 1, T0], [T0 + pf.FREIGABE_MS, T0], [T0 + pf.FREIGABE_MS + 1, T0],
  [NaN, T0], [0, T0], [-5, T0], ["123", T0], [null, T0],
];
const ROH = ["", "abc", "0", "123", "12345678901234567", "1700000000000", " 1", null, "1e5"];
for (const name of ["tablet.html", "anzeige.html", "lobby.html"]) {
  const text = seite(name);
  const quellen = ["neueFreigabe", "freigabeGueltig", "freigabeLesen"].map((n) => funktion(text, n));
  ok(`${name}: PIN-Freigabe-Kopien vorhanden`, quellen.every(Boolean), true);
  ok(`${name}: Freigabe-Schlüssel`, zeichenkette(text, "PIN_FREIGABE_KEY"), "badhub.tablet.pinFreigabeBis");
  if (!quellen.every(Boolean)) continue;
  const frist = text.match(/(?:var|const) FREIGABE_MS = ([^;]+);/);
  ok(`${name}: Frist wie im Modul`, frist ? Function("return " + frist[1])() : null, pf.FREIGABE_MS);
  // eslint-disable-next-line no-new-func
  const f = new Function("var FREIGABE_MS = " + pf.FREIGABE_MS + ";\n" + quellen.join("\n") +
    "\nreturn { neueFreigabe, freigabeGueltig, freigabeLesen };")();
  ok(`${name}: neueFreigabe wie Modul`, f.neueFreigabe(T0), pf.neueFreigabe(T0));
  ok(`${name}: freigabeGueltig wie Modul`,
    FREIGABE_FAELLE.map(([b, j]) => f.freigabeGueltig(b, j)), FREIGABE_FAELLE.map(([b, j]) => pf.freigabeGueltig(b, j)));
  ok(`${name}: freigabeLesen wie Modul`, ROH.map(f.freigabeLesen), ROH.map(pf.freigabeLesen));
}

// ── Schlankes Undo-Format (src/io/undoStand.mjs, ADR 0084) ──
// Eine abweichende Kopie stellte nach „zurück" einen falschen Punktverlauf
// wieder her oder ließe den Spiegel über den Deckel wachsen — beides fiele
// erst beim Gerätewechsel mitten im Spiel auf.
{
  const text = seite("tablet.html");
  const namen = ["utf8Laenge", "verlaufGueltig", "verlaufSchnitt", "verlaufAufSchnitt", "verlaufAusStand", "verlaufIstPraefix", "historyNormalisieren", "spiegelKuerzen"];
  const quellen = namen.map((n) => funktion(text, n));
  ok("tablet.html: Undo-Stand-Kopien vorhanden", quellen.every(Boolean), true);
  const budget = text.match(/const SPIEGEL_BUDGET = (\d+);/);
  ok("tablet.html: SPIEGEL_BUDGET wie Modul", budget ? Number(budget[1]) : null, us.SPIEGEL_BUDGET);
  if (quellen.every(Boolean)) {
    // eslint-disable-next-line no-new-func
    const f = new Function(quellen.join("\n") + "\nreturn { " + namen.join(", ") + " };")();
    const L = (...p) => ({ sets: p.map((x, i) => ({ startA: i, startB: 0, points: x })), midGame: false });
    const LOGS = [null, L(), L("A"), L("AAB", "B"), L("AABAB", "BA", "A"), { ...L("AB"), midGame: true },
      7, "x", {}, { sets: [null] }, { sets: [{}] }, { sets: [{ points: 3 }] }];
    const SCHNITTE = [null, [[], false], [[1], false], [[3, 1], true], [[9], false], [[2, 0, 0], false],
      7, [7], [[-1], false], [[1.5], false], [["1"], false]];
    const TEXTE = ["", "abc", "Müller", "李娜", "😀x"];
    ok("tablet.html: utf8Laenge wie Modul", TEXTE.map(f.utf8Laenge), TEXTE.map(us.utf8Laenge));
    ok("tablet.html: verlaufGueltig wie Modul", LOGS.map(f.verlaufGueltig), LOGS.map(us.verlaufGueltig));
    ok("tablet.html: verlaufSchnitt wie Modul", LOGS.map(f.verlaufSchnitt), LOGS.map(us.verlaufSchnitt));
    const paare = LOGS.flatMap((l) => SCHNITTE.map((r) => [l, r]));
    ok("tablet.html: verlaufAufSchnitt wie Modul",
      paare.map(([l, r]) => f.verlaufAufSchnitt(l, r)), paare.map(([l, r]) => us.verlaufAufSchnitt(l, r)));
    const STAENDE = [{}, { rl: null }, { rl: [[1], false] }, { rallyLog: L("B"), rl: [[1], false] }, { rallyLog: null }];
    const sl = STAENDE.flatMap((st) => LOGS.map((l) => [st, l]));
    ok("tablet.html: verlaufAusStand wie Modul",
      sl.map(([st, l]) => f.verlaufAusStand(st, l)), sl.map(([st, l]) => us.verlaufAusStand(st, l)));
    const ll = LOGS.flatMap((x) => LOGS.map((y) => [x, y]));
    ok("tablet.html: verlaufIstPraefix wie Modul",
      ll.map(([x, y]) => f.verlaufIstPraefix(x, y)), ll.map(([x, y]) => us.verlaufIstPraefix(x, y)));
    const HIST = [
      [[], null],
      [[{ rallyLog: L("A"), events: [1] }, { rallyLog: L("AA"), events: [1, 2] }], L("AAB")],
      [[{ rallyLog: L("B") }, { rallyLog: L("AB"), korrektur: { art: "ab" } }, { rl: [[2], false] }], L("ABA")],
      [[{ x: 1 }, { rallyLog: null }, { rallyLog: L("A") }], null],
      [[{ rallyLog: L("AB"), korrektur: { art: "seiten" } }], L("AB")],
      [null, null], [[null, 3, { a: 1 }, { rallyLog: { sets: [null] } }], { sets: [{}] }],
      [JSON.parse('[{"a":1,"__proto__":{"rl":[[9],false]}}]'), L("AB")],
    ];
    ok("tablet.html: historyNormalisieren wie Modul",
      HIST.map(([h, l]) => f.historyNormalisieren(h, l)), HIST.map(([h, l]) => us.historyNormalisieren(h, l)));
    const gross = { m: 1, history: Array.from({ length: 30 }, (_, i) => ({ i, t: "xü李".repeat(130) })) };
    const gj = JSON.stringify(gross);
    const B = [100000, 6000, 500, 10];
    ok("tablet.html: spiegelKuerzen wie Modul",
      B.map((b) => f.spiegelKuerzen(gross, gj, b)), B.map((b) => us.spiegelKuerzen(gross, gj, b)));
  }
  // Die Aufrufstellen: Undo-Stand ohne Vollkopien, Korrektur mit.
  const snap = funktion(text, "snapshot") || "";
  ok("tablet.html: snapshot() schreibt den Schnitt", /rl: verlaufSchnitt\(STATE\.rallyLog\)/.test(snap), true);
  ok("tablet.html: snapshot() kopiert weder Verlauf noch Ereignisse", /rallyLog:|events:/.test(snap), false);
  const kor = funktion(text, "fuehreKorrekturAus") || "";
  ok("tablet.html: Mannschaftstausch behält die Vollkopie", /art === 'ab'[\s\S]*snap\.rallyLog = [\s\S]*delete snap\.rl/.test(kor), true);
  const rest = funktion(text, "restoreSnapshot") || "";
  ok("tablet.html: restoreSnapshot nutzt verlaufAusStand", /STATE\.rallyLog = verlaufAusStand\(snap, STATE\.rallyLog\)/.test(rest), true);
  const app = funktion(text, "applyPersistedState") || "";
  const hn = app.indexOf("const historyNeu = historyNormalisieren(p.history, verlaufNeu);");
  const ersteZuweisung = app.search(/\n\s*STATE\.\w+ = /);
  ok("tablet.html: geladene Stände werden normalisiert — vor der ersten Zuweisung",
    hn > 0 && ersteZuweisung > hn && app.includes("STATE.history = historyNeu;") && app.includes("STATE.rallyLog = verlaufNeu;"), true);
}

// ── Geparkte Ergebnisse, Retry-Regel, Beendet-Ansicht (Spec tablet-stand-robust, Regel C) ──
// Eine abweichende Kopie parkte nicht (das Folgespiel bliebe blockiert), löste
// zu früh auf (ein Ergebnis ginge verloren) oder läse „überholt" als Annahme.
{
  const text = seite("tablet.html");
  const namen = ["geparktGueltig", "geparktLaden", "mussParken", "parken", "aufloesen", "geparktFuer", "zuPruefen", "freigeben",
    "geparktSichtbar", "naechstePruefungMs", "saetzeText", "spielName", "bandText", "frageText", "verwerfenText"];
  const quellen = namen.map((n) => funktion(text, n));
  ok("tablet.html: Park-Kopien vorhanden", quellen.every(Boolean), true);
  const KONST = ["GEPARKT_MAX", "GEPARKT_MAX_SAETZE", "GEPARKT_ERSTE_PRUEFUNG_MS", "GEPARKT_TAKT_MS", "GEPARKT_KARENZ_MS"];
  const werte = KONST.map((k) => { const m = text.match(new RegExp("const " + k + " = (\\d+);")); return m ? Number(m[1]) : null; });
  ok("tablet.html: Park-Konstanten wie Modul", werte, KONST.map((k) => gp[k]));
  if (quellen.every(Boolean) && werte.every((w) => w != null)) {
    // eslint-disable-next-line no-new-func
    const f = new Function(KONST.map((k, i) => `const ${k} = ${werte[i]};`).join("\n") + "\n" + quellen.join("\n")
      + "\nreturn { " + namen.join(", ") + " };")();
    const A = (id, extra) => ({ matchId: id, sets: [{ a: 21, b: 10 }, { a: 5, b: 2 }], ...extra });
    const AUFTRAEGE = [A(7), A(8, { retired: true }), A(9, { walkover: true, sets: [] }), { matchId: 3, sets: [null, { a: "x" }] }];
    const EINTRAEGE = AUFTRAEGE.map((a, i) => ({ auftrag: a, matchNr: i % 2 ? null : 10 + i, seitMs: 1000 * i }));
    const LISTEN = [[], EINTRAEGE.slice(0, 1), EINTRAEGE.slice(0, 2), EINTRAEGE,
      Array.from({ length: 8 }, (_, i) => ({ auftrag: A(100 + i), matchNr: i, seitMs: i }))];
    const SCHMUTZ = [null, 3, "x", {}, [], { auftrag: null }, { auftrag: [] }, { auftrag: { matchId: "7", sets: [] } },
      { auftrag: { matchId: 7 } }, { auftrag: { matchId: NaN, sets: [] } }, EINTRAEGE[0],
      { auftrag: { matchId: 7, sets: Array(7).fill(null) } }, { auftrag: { matchId: 7, sets: Array(8).fill(null) } }];
    const ROH = [null, undefined, "", "kaputt{", "3", '{"a":1}', JSON.stringify(SCHMUTZ), JSON.stringify(EINTRAEGE),
      JSON.stringify([...EINTRAEGE, ...EINTRAEGE]), JSON.stringify(LISTEN[4])];
    const IDS = [7, 8, 99, null, undefined, 100];
    const gleich = (name, faelle, ruf) =>
      ok("tablet.html: " + name + " wie Modul", faelle.map((x) => ruf(f, x)), faelle.map((x) => ruf(gp, x)));
    gleich("geparktGueltig", SCHMUTZ, (m, x) => m.geparktGueltig(x));
    gleich("geparktLaden", ROH, (m, x) => m.geparktLaden(x));
    gleich("mussParken", [null, undefined, 3, ...AUFTRAEGE].flatMap((a) => IDS.map((i) => [a, i])), (m, [a, i]) => m.mussParken(a, i));
    gleich("parken", LISTEN.flatMap((l) => AUFTRAEGE.map((a) => [l, a])).flatMap((x) => [[...x, 12], [...x, "A3"], [...x, undefined], [...x, {}]]),
      (m, [l, a, nr]) => m.parken(l, a, nr, 5000));
    const li = LISTEN.flatMap((l) => IDS.map((i) => [l, i]));
    gleich("aufloesen", li, (m, [l, i]) => m.aufloesen(l, i));
    gleich("geparktFuer", li, (m, [l, i]) => m.geparktFuer(l, i));
    gleich("zuPruefen", li, (m, [l, i]) => m.zuPruefen(l, i));
    gleich("freigeben", li, (m, [l, i]) => m.freigeben(l, i));
    gleich("zuPruefen nach Freigabe", li, (m, [l, i]) => m.zuPruefen(m.freigeben(l, i), i));
    gleich("geparktSichtbar", LISTEN.flatMap((l) => [0, 3999, 4000, 5500, 1e9].map((j) => [l, j])).concat([[[{ auftrag: A(1) }], 0]]),
      (m, [l, j]) => m.geparktSichtbar(l, j));
    gleich("naechstePruefungMs", [true, false, 0, 1, undefined], (m, x) => m.naechstePruefungMs(x));
    gleich("saetzeText", [...AUFTRAEGE, {}, null, { sets: 3 }], (m, x) => m.saetzeText(x));
    gleich("spielName", [...EINTRAEGE, { matchNr: "" }, { matchNr: 0 }, {}], (m, x) => m.spielName(x));
    gleich("bandText", LISTEN, (m, x) => m.bandText(x));
    gleich("frageText", EINTRAEGE, (m, x) => m.frageText(x));
    gleich("verwerfenText", EINTRAEGE, (m, x) => m.verwerfenText(x));
  }

  // Retry-Regel: jede Kombination der drei Felder plus Unsinn.
  const ns = funktion(text, "naechsterSchritt");
  ok("tablet.html: naechsterSchritt vorhanden", !!ns, true);
  if (ns) {
    // eslint-disable-next-line no-new-func
    const kopie = new Function("RESULT_OHNE_GRUND", ns + "\nreturn naechsterSchritt;")(rr.OHNE_GRUND);
    const W = [true, false, 1, undefined];
    const ANTW = [null, undefined, "x", 0, {}, { error: "  " }, { error: 42 }]
      .concat(W.flatMap((o) => W.flatMap((p) => W.map((s) => ({ ok: o, permanent: p, superseded: s, error: " Grund " })))));
    ok("tablet.html: naechsterSchritt wie Modul", ANTW.map(kopie), ANTW.map(rr.naechsterSchritt));
  }

  // Beendet-Ansicht: alle Lagen.
  const al = funktion(text, "abschlussLage");
  ok("tablet.html: abschlussLage vorhanden", !!al, true);
  if (al) {
    // eslint-disable-next-line no-new-func
    const kopie = new Function(al + "\nreturn abschlussLage;")();
    const B = [false, true];
    const LAGEN = [null, undefined].concat(B.flatMap((fi) => B.flatMap((ue) => B.flatMap((so) => [null, "Satz 13:8"].flatMap(
      (rj) => B.flatMap((pr) => B.map((nw) => ({ finalized: fi, ueberholt: ue, submittedOk: so, resultRejected: rj, pendingResult: pr, needWinner: nw }))))))));
    ok("tablet.html: abschlussLage wie Modul", LAGEN.map(kopie), LAGEN.map(aa.abschlussLage));
  }
}

if (failures) {
  console.error(`${failures} Test(s) fehlgeschlagen`);
  process.exit(1);
}
