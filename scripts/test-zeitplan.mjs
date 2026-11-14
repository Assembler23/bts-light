// Testet die Texte zur geplanten Zeit (src/io/zeitplan.mjs, Spec tl-zeitplan)
// und schneidet die Inline-Kopien aus tl.html, um sie auf denselben Eingaben
// gegen das Modul laufen zu lassen. Eine abweichende Kopie zeigte der
// Turnierleitung eine falsche Planzeit — und daran liest sie ab, ob das
// Turnier im Zeitplan liegt.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import * as modul from "../src/io/zeitplan.mjs";

const { planZeitText, startAbweichungText, zeitplanLage, panelStandardSichtbar } = modul;

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

// ── planZeitText ──
// 15.11.2026 ist ein Sonntag, 14.11.2026 ein Samstag.
ok("heute nur Uhrzeit", planZeitText(202611141020, 20261114), "10:20");
ok("führende Nullen", planZeitText(202611140905, 20261114), "09:05");
ok("morgen mit Wochentag", planZeitText(202611150900, 20261114), "So 09:00");
ok("gestern mit Wochentag", planZeitText(202611131830, 20261114), "Fr 18:30");
ok("ohne heute nur Uhrzeit", planZeitText(202611150900, 0), "09:00");
ok("heute fehlt", planZeitText(202611150900, undefined), "09:00");
ok("ohne Planzeit", planZeitText(null, 20261114), "");
ok("fehlende Planzeit", planZeitText(undefined, 20261114), "");
ok("Unsinn", planZeitText("abc", 20261114), "");
ok("zu kurz", planZeitText(2026111410, 20261114), "");
ok("Stunde 24", planZeitText(202611142400, 20261114), "");
ok("Monat 0", planZeitText(202600141020, 20261114), "");
ok("Mitternacht", planZeitText(202611140000, 20261114), "00:00");

// ── startAbweichungText ──
ok("später", startAbweichungText(18), "+18 min");
ok("früher", startAbweichungText(-4), "−4 min");
ok("pünktlich", startAbweichungText(0), "±0 min");
ok("fehlt (null)", startAbweichungText(null), "");
ok("fehlt (undefined)", startAbweichungText(undefined), "");
ok("Unsinn", startAbweichungText("x"), "");

// ── zeitplanLage ──
const M = 60000;
const PLAN = 1_800_000_000_000; // Planzeit 11:00 als Unix-ms (Zone egal)
const fenster = (extra) => ({
  kind: "ok", started: true, planned_time: 202611141100, planned_ms: PLAN,
  predicted_start_ms: PLAN, predicted_uncertain: false, ...extra,
});
const FRUEH = PLAN - 3 * 3600 * 1000; // „jetzt" weit vor allem
const lage = (extra, now = FRUEH) => zeitplanLage(fenster(extra), now);

ok("pünktlich", lage({}), {
  ton: "ok", text: "Im Zeitplan", neben: "Nächstes Zeitfenster 11:00 · voraussichtlich 11:00" });
ok("4 min später", lage({ predicted_start_ms: PLAN + 4 * M }).text, "Im Zeitplan");
ok("5:59 später", lage({ predicted_start_ms: PLAN + 5 * M + 59000 }).text, "Im Zeitplan");
ok("6:00 später", lage({ predicted_start_ms: PLAN + 6 * M }), {
  ton: "spaet", text: "6 min dahinter",
  neben: "Nächstes Zeitfenster 11:00 · voraussichtlich 11:06" });
ok("18 min später", lage({ predicted_start_ms: PLAN + 18 * M }).text, "18 min dahinter");
ok("5:59 früher", lage({ predicted_start_ms: PLAN - 5 * M - 59000 }).text, "Im Zeitplan");
ok("6:00 früher", lage({ predicted_start_ms: PLAN - 6 * M }), {
  ton: "frueh", text: "6 min davor",
  neben: "Nächstes Zeitfenster 11:00 · voraussichtlich 10:54" });
ok("Pause: 50 min davor", lage({ predicted_start_ms: PLAN - 50 * M }).text, "50 min davor");
ok("unsichere Prognose", lage({ predicted_start_ms: PLAN + 18 * M, predicted_uncertain: true }).neben,
  "Nächstes Zeitfenster 11:00 · voraussichtlich ca. 11:18");
ok("über Mitternacht",
  zeitplanLage(fenster({ planned_time: 202611142350, predicted_start_ms: PLAN + 20 * M }), FRUEH).neben,
  "Nächstes Zeitfenster 23:50 · voraussichtlich 00:10");

// Klemme: Die Prognose ist alt (Revision unbewegt), die Uhr läuft weiter.
ok("alte Prognose, Uhr 3 min nach Plan", lage({}, PLAN + 3 * M).text, "Im Zeitplan");
ok("alte Prognose, Uhr 9 min nach Plan", lage({}, PLAN + 9 * M).text, "9 min dahinter");
ok("Rückstand wächst mit", lage({ predicted_start_ms: PLAN + 7 * M }, PLAN + 12 * M).text, "12 min dahinter");
ok("frische Prognose gewinnt", lage({ predicted_start_ms: PLAN + 20 * M }, PLAN + 12 * M).text, "20 min dahinter");

// Vor dem ersten Spiel des Tages.
ok("Beginn weit vorher", lage({ started: false }), { ton: "neutral", text: "Beginn 11:00", neben: "" });
ok("Beginn bis 5 min danach", lage({ started: false }, PLAN + 5 * M).text, "Beginn 11:00");
ok("ab 6 min ohne Start: dahinter", lage({ started: false }, PLAN + 6 * M).text, "6 min dahinter");
ok("Beginn ohne Prognose", lage({ started: false, kind: "no_forecast", predicted_start_ms: undefined }).text,
  "Beginn 11:00");
ok("Beginn bei Prognose aus", lage({ started: false, kind: "off" }).text, "Beginn 11:00");
ok("ohne Start, ohne Prognose, überfällig",
  lage({ started: false, kind: "no_forecast", predicted_start_ms: undefined }, PLAN + 10 * M),
  { ton: "neutral", text: "Noch keine Prognose", neben: "Nächstes Zeitfenster 11:00" });

// Neutrale Zustände.
ok("keine Planzeiten", zeitplanLage({ kind: "no_plan" }, FRUEH),
  { ton: "neutral", text: "Keine geplanten Zeiten in BTP", neben: "" });
ok("heute fertig", zeitplanLage({ kind: "done_today", started: true }, FRUEH).text, "Heute keine Spiele mehr");
ok("heute fertig, nichts begonnen", zeitplanLage({ kind: "done_today", started: false }, FRUEH).text,
  "Heute keine Spiele mehr");
ok("letzte Runde", zeitplanLage({ kind: "last_round", started: true }, FRUEH),
  { ton: "neutral", text: "Letzte Runde läuft", neben: "" });
ok("Prognose aus", lage({ kind: "off", predicted_start_ms: undefined }),
  { ton: "neutral", text: "Prognose ist ausgeschaltet", neben: "Nächstes Zeitfenster 11:00" });
ok("keine Prognose", lage({ kind: "no_forecast", predicted_start_ms: undefined }),
  { ton: "neutral", text: "Noch keine Prognose", neben: "Nächstes Zeitfenster 11:00" });
ok("ok ohne Wert", lage({ predicted_start_ms: undefined }).text, "Noch keine Prognose");
ok("ok ohne Planzeit-ms", lage({ planned_ms: undefined }).text, "Noch keine Prognose");
ok("unbekannte Kennung", lage({ kind: "neu" }).text, "Noch keine Prognose");
ok("fehlt", zeitplanLage(null, FRUEH), { ton: "neutral", text: "", neben: "" });
ok("kaputt", zeitplanLage("x", FRUEH), { ton: "neutral", text: "", neben: "" });
ok("Uhr kaputt", lage({}, NaN).text, "Noch keine Prognose");

// ── Panel standardmäßig unsichtbar ──
ok("zeitplan fehlt ⇒ unsichtbar", panelStandardSichtbar("zeitplan"), false);
ok("andere fehlen ⇒ sichtbar", ["courts", "queue", "checkin", "neu"].map(panelStandardSichtbar),
  [true, true, true, true]);

// ── Inline-Kopien in tl.html ──
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

function kopieVon(name) {
  const quelle = funktion(tl, name);
  ok(`tl.html trägt die Kopie von ${name}`, !!quelle, true);
  // eslint-disable-next-line no-new-func
  return quelle ? new Function(quelle + `\nreturn ${name};`)() : null;
}

const planKopie = kopieVon("planZeitText");
if (planKopie) {
  const faelle = [];
  for (const heute of [20261114, 20261115, 20270101, 0, undefined, "x"]) {
    for (const pt of [202611141020, 202611150900, 202612312359, 202701010000,
      202611142400, 202613011000, 2026, null, undefined, "abc", NaN]) {
      faelle.push([pt, heute]);
    }
  }
  ok("planZeitText: Kopie verhält sich wie das Modul",
    faelle.map(([p, h]) => planKopie(p, h)),
    faelle.map(([p, h]) => planZeitText(p, h)));
}
const abwKopie = kopieVon("startAbweichungText");
if (abwKopie) {
  const faelle = [-721, -60, -5, -1, 0, 1, 5, 18, 720, 1.9, -1.9, null, undefined, "", "x", NaN];
  ok("startAbweichungText: Kopie verhält sich wie das Modul",
    faelle.map((m) => abwKopie(m)),
    faelle.map((m) => startAbweichungText(m)));
}

// `zeitplanLage` ruft `planZeitText` — die Kopie bekommt beide Funktionen.
const lageQuelle = funktion(tl, "zeitplanLage");
const planQuelle = funktion(tl, "planZeitText");
ok("tl.html trägt die Kopie von zeitplanLage", !!lageQuelle, true);
if (lageQuelle && planQuelle) {
  // eslint-disable-next-line no-new-func
  const lageKopie = new Function(`${planQuelle}\n${lageQuelle}\nreturn zeitplanLage;`)();
  const faelle = [];
  for (const kind of ["ok", "off", "no_forecast", "last_round", "done_today", "no_plan", "neu"]) {
    for (const started of [true, false]) {
      for (const d of [-60, -6, -5, 0, 5, 6, 18, 700]) {
        for (const now of [FRUEH, PLAN + 3 * M, PLAN + 9 * M, NaN]) {
          faelle.push([fenster({ kind, started, predicted_start_ms: PLAN + d * M + 59000,
            predicted_uncertain: d === 18 }), now]);
        }
      }
    }
  }
  faelle.push([null, FRUEH], [{}, FRUEH], [fenster({ planned_ms: undefined }), FRUEH],
    [fenster({ planned_time: undefined }), FRUEH], [fenster({ predicted_start_ms: null }), FRUEH]);
  ok("zeitplanLage: Kopie verhält sich wie das Modul",
    faelle.map(([s, n]) => lageKopie(s, n)),
    faelle.map(([s, n]) => zeitplanLage(s, n)));
}

// Die Regel „standardmäßig unsichtbar" und ihre Wirkung in `normalizedPanels`.
const konstante = /const PANEL_STANDARD_AUS = (\[[^\]]*\]);/.exec(tl);
ok("tl.html trägt PANEL_STANDARD_AUS", konstante && JSON.parse(konstante[1]), modul.PANEL_STANDARD_AUS);
const keys = /const PANEL_KEYS = (\[[^\]]*\]);/.exec(tl);
const sichtbarQuelle = funktion(tl, "panelStandardSichtbar");
const normQuelle = funktion(tl, "normalizedPanels");
ok("tl.html trägt normalizedPanels", !!(keys && sichtbarQuelle && normQuelle && konstante), true);
if (keys && sichtbarQuelle && normQuelle && konstante) {
  // eslint-disable-next-line no-new-func
  const normalizedPanels = new Function(
    `const PANEL_KEYS = ${keys[1]};\nconst PANEL_STANDARD_AUS = ${konstante[1]};\n`
    + `${sichtbarQuelle}\n${normQuelle}\nreturn normalizedPanels;`)();
  const sicht = (panels) => Object.fromEntries(
    normalizedPanels({ panels }).map((p) => [p.key, p.visible]));
  const bestand = sicht([
    { key: "courts", visible: true, column: 1 },
    { key: "queue", visible: true, column: 2 },
  ]);
  ok("Bestandsprofil: zeitplan unsichtbar", bestand.zeitplan, false);
  ok("Bestandsprofil: übrige fehlende sichtbar", [bestand.finished, bestand.times, bestand.checkin],
    [true, true, true]);
  ok("Bestandsprofil: vorhandene unverändert", [bestand.courts, bestand.queue], [true, true]);
  ok("eingeschaltet bleibt sichtbar", sicht([{ key: "zeitplan", visible: true, column: 1 }]).zeitplan, true);
  ok("ausgeschaltet bleibt aus", sicht([{ key: "zeitplan", visible: false, column: 1 }]).zeitplan, false);
  ok("leeres Profil", sicht(undefined).zeitplan, false);
}

if (failures) {
  console.error(`\n${failures} Fehler`);
  process.exit(1);
}
console.log("\nalle Tests grün");
