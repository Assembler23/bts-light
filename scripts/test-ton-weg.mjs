// Testet die reine Tonweg-Logik (src/io/tonWeg.mjs) — DASSELBE Modul, das
// announcer.ts benutzt, um Gong und Stimme über den Rust-Kern abzuspielen
// (Spec `ansage-ausgabegeraet`, ADR 0065):
//  A) Fehlercodes aus "<code>: <Text>" lesen
//  B) welcher Fehler welches Banner auslöst (Linux/Abbruch: keins)
//  C) Fristen für PCM-Stück und Windows-Stimme
//  D) mitFrist: Ergebnis, Fehler, Frist — genau eine Auflösung
//  E) vertonenMitFrist: Cloud-Vertonung höchstens harte Frist + 0,5 s (Spec
//     `ansage-offline-stimme`, K10), Fehlertext des Kerns bleibt erhalten
import {
  fehlerCode,
  bannerFuer,
  pcmFristMs,
  stimmeFristMs,
  mitFrist,
  abschnitteTeilen,
  VERTONUNG_FRIST_MS,
  VERTONUNG_FRIST_TEXT,
  vertonenMitFrist,
} from "../src/io/tonWeg.mjs";
import { readFileSync } from "node:fs";

let failures = 0;
function check(name, cond) {
  if (!cond) {
    console.error(`✗ ${name}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

// A) Fehlercodes
check("Code vor dem Doppelpunkt", fehlerCode("kein_geraet: nichts da") === "kein_geraet");
check("Code ohne Text", fehlerCode("abgebrochen") === "abgebrochen");
check("Error-Objekt", fehlerCode(new Error("keine_stimme: de fehlt")) === "keine_stimme");
check("unbekanntes Format → intern", fehlerCode("irgendwas kaputt") === "intern");
check("unbekannter Code → intern", fehlerCode("quatsch: x") === "intern");
check("null → intern", fehlerCode(null) === "intern");

// B) Banner
check("Linux: kein Banner", bannerFuer("nicht_unterstuetzt") === null);
check("Abbruch: kein Banner", bannerFuer("abgebrochen") === null);
check("kein Gerät → kein_geraet", bannerFuer("kein_geraet") === "kein_geraet");
check("keine Stimme → keine_stimme", bannerFuer("keine_stimme") === "keine_stimme");
check("Frist → rueckfall", bannerFuer("frist") === "rueckfall");
check("intern → rueckfall", bannerFuer("intern") === "rueckfall");
check("ungueltig → rueckfall", bannerFuer("ungueltig") === "rueckfall");

// C) Fristen — müssen EINE Wiederholung nach einem Abriss abdecken
// (Stück von vorn, D8), sonst bricht die Frist genau die Rettung ab.
check("PCM: 1 s Ton → 2×1000 + 5000", pcmFristMs(24000, 24000) === 7000);
check("PCM: Rate 0 → nur Sockel", pcmFristMs(100, 0) === 5000);
{
  const seg = ["Feld 3.", "Herreneinzel."]; // 20 Zeichen, 2 Abschnitte
  const schaetz = Math.ceil(20 * 140 * (0.8 / 0.8) + 2 * 300 + 170);
  check("Stimme bei Rate 0,8", stimmeFristMs(seg, 0.8) === 2 * schaetz + 5000);
  check(
    "Stimme bei Rate 0,5 länger als bei 0,8",
    stimmeFristMs(seg, 0.5) > stimmeFristMs(seg, 0.8),
  );
}
check("Stimme: leere Liste", stimmeFristMs([], 0.8) === 2 * 170 + 5000);
check("Stimme: Rate ungültig → wie 0,8", stimmeFristMs(["ab"], NaN) === stimmeFristMs(["ab"], 0.8));

// C2) lange Abschnitte teilen statt kappen (Kern nimmt je ≤ 1000 Zeichen)
{
  const satz = "Das ist ein Satz. ";
  const lang = satz.repeat(80).trim(); // ~1440 Zeichen
  const teile = abschnitteTeilen([lang, "kurz"], 1000);
  check("langer Text wird geteilt", teile.length === 3);
  check("kein Teil über 1000", teile.every((t) => t.length <= 1000));
  check("nichts geht verloren", teile.join(" ").replace(/\s+/g, " ") === (lang + " kurz").replace(/\s+/g, " "));
  check("Schnitt am Satzende", teile[0].endsWith("."));
  const wurst = "x".repeat(2500);
  const harte = abschnitteTeilen([wurst], 1000);
  check("ohne Leerzeichen hart geteilt", harte.length === 3 && harte.join("") === wurst);
  check("leere fallen weg", abschnitteTeilen(["", "  ", "a"], 1000).length === 1);
}

// D) mitFrist mit virtueller Uhr
function makeClock() {
  const timers = [];
  return {
    setTimeoutFn: (cb, ms) => {
      timers.push({ ms, cb });
      return timers.length;
    },
    clearTimeoutFn: () => {},
    feuern: () => timers.splice(0).forEach((t) => t.cb()),
    letzteMs: () => timers[timers.length - 1]?.ms,
  };
}

const c1 = makeClock();
const r1 = await mitFrist(Promise.resolve(42), 1000, c1);
check("Ergebnis vor der Frist", r1.ok === true && r1.wert === 42);

const c2 = makeClock();
const r2 = await mitFrist(Promise.reject(new Error("kein_geraet: weg")), 1000, c2);
check("Fehler vor der Frist", r2.ok === false && r2.code === "kein_geraet");

const c3 = makeClock();
const haengt = new Promise(() => {});
const p3 = mitFrist(haengt, 1000, c3);
c3.feuern();
const r3 = await p3;
check("Frist läuft ab", r3.ok === false && r3.code === "frist");

const c4 = makeClock();
let spaet;
const p4 = mitFrist(new Promise((r) => (spaet = r)), 1000, c4);
c4.feuern();
spaet(7);
const r4 = await p4;
check("nach der Frist zählt das späte Ergebnis nicht", r4.code === "frist");

// E) vertonenMitFrist
const rust = readFileSync(new URL("../src-tauri/src/tts_netzlage.rs", import.meta.url), "utf8");
const harteFrist = rust.match(/HARTE_FRIST: Duration = Duration::from_secs\((\d+)\)/);
check(
  "Frist = harte Frist des Kerns + 500 ms",
  harteFrist && Number(harteFrist[1]) * 1000 + 500 === VERTONUNG_FRIST_MS,
);

const c5 = makeClock();
check("Vertonung vor der Frist", (await vertonenMitFrist(Promise.resolve("b64"), c5)) === "b64");

const c6 = makeClock();
let fehler6 = null;
await vertonenMitFrist(Promise.reject("Azure: keine Verbindung zum Sprachdienst"), c6).catch(
  (e) => (fehler6 = e),
);
check("Kern-Fehler bleibt erhalten", fehler6 === "Azure: keine Verbindung zum Sprachdienst");

const c7 = makeClock();
let fehler7 = null;
const p7 = vertonenMitFrist(new Promise(() => {}), c7).catch((e) => (fehler7 = e));
check("Frist gestellt", c7.letzteMs() === VERTONUNG_FRIST_MS);
c7.feuern();
await p7;
check("hängender Kern → Fehler nach der Frist", fehler7?.message === VERTONUNG_FRIST_TEXT);

const c8 = makeClock();
let spaet8;
let fehler8 = null;
const p8 = vertonenMitFrist(new Promise((r) => (spaet8 = r)), c8).catch((e) => (fehler8 = e));
c8.feuern();
spaet8("zu spät");
check("nach der Frist zählt die späte Vertonung nicht", (await p8) === fehler8 && fehler8 !== null);

if (failures) {
  console.error(`\n${failures} Fehler`);
  process.exit(1);
}
console.log("\nalle Tonweg-Tests grün");
