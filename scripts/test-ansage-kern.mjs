// Testet den Abspiel-Kern der Ansage-Seiten (src/io/ansageKern.mjs) mit
// Attrappen für fetch, AudioContext, Sprachausgabe und Uhr — und hält die
// Inline-Kopien der drei Ansage-Module in allen vier Assets wortgleich
// (ansage.html, monitor.html, overview.html, combo.html; Spec
// ansage-im-browser, E1b).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import {
  ansageKern, ansageModusAusQuery, ansageImLan, monitorHinweis,
  AUDIO_LADE_FRIST_MS, ANSAGE_FEED_FRIST_MS, ANSAGE_FEHLT_PAUSE_MS,
} from "../src/io/ansageKern.mjs";
import { ANSAGE_TAKT_MS } from "../src/io/ansageSeite.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

// ── Attrappen ───────────────────────────────────────────────────────────
function welt({ tonLaeuft = true, stimmen = [] } = {}) {
  let uhr = 1_000_000;
  let timerNr = 0;
  const timer = new Map();
  const abrufe = [];
  const gesprochen = [];
  const gespielt = [];
  const antworten = []; // Warteschlange von Antwort-Erzeugern je Feed-Abruf
  const audioAntwort = { art: "ok" };

  class Ctx {
    constructor() { this.state = tonLaeuft ? "running" : "suspended"; this.destination = {}; }
    get currentTime() { return uhr / 1000; }
    resume() { this.state = "running"; return Promise.resolve(); }
    createGain() {
      return { gain: { setValueAtTime() {}, exponentialRampToValueAtTime() {} }, connect() {} };
    }
    createOscillator() {
      return { frequency: {}, connect() {}, start() {}, stop() {}, onended: null };
    }
    decodeAudioData(buf, ok) { ok({ duration: 1 }); }
    createBufferSource() {
      return {
        connect() {},
        start() { gespielt.push(this.kennung); },
        set buffer(_) {},
        onended: null,
      };
    }
  }

  const w = {
    Date: { now: () => uhr },
    AudioContext: Ctx,
    AbortController: class { constructor() { this.signal = { aborted: false, h: [] }; } abort() { this.signal.aborted = true; this.signal.h.forEach((f) => f()); } },
    speechSynthesis: {
      getVoices: () => stimmen,
      speak(u) { gesprochen.push(u.text || u); w.setTimeout(() => u.onend && u.onend(), 10); },
    },
    SpeechSynthesisUtterance: class { constructor(t) { this.text = t; } },
    setTimeout(fn, ms) { const id = ++timerNr; timer.set(id, { fn, bis: uhr + (ms || 0) }); return id; },
    clearTimeout(id) { timer.delete(id); },
    setInterval(fn, ms) {
      const id = ++timerNr;
      const wieder = () => { timer.set(id, { fn: () => { fn(); wieder(); }, bis: uhr + ms }); };
      wieder();
      return id;
    },
    fetch(url, opt) {
      abrufe.push(url);
      if (url.includes("/ansage/audio/")) {
        if (audioAntwort.art === "haengt") {
          return new Promise((_, nein) => {
            opt.signal.h.push(() => nein(new Error("abgebrochen")));
          });
        }
        if (audioAntwort.art === "404") return Promise.resolve({ ok: false, status: 404 });
        return Promise.resolve({ ok: true, status: 200, arrayBuffer: () => Promise.resolve(new ArrayBuffer(4)) });
      }
      const erzeuger = antworten.shift();
      if (!erzeuger) {
        return new Promise((_, nein) => { opt.signal.h.push(() => nein(new Error("abgebrochen"))); });
      }
      return erzeuger();
    },
  };
  const flush = () => new Promise((r) => setImmediate(r));
  return {
    w, abrufe, gesprochen, gespielt, antworten, audioAntwort,
    feed(feed) { antworten.push(() => Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve(feed) })); },
    status(code) { antworten.push(() => Promise.resolve({ ok: false, status: code })); },
    async lauf(ms) {
      const ziel = uhr + ms;
      for (;;) {
        await flush(); await flush();
        let naechster = null;
        for (const [id, t] of timer) if (t.bis <= ziel && (!naechster || t.bis < naechster[1].bis)) naechster = [id, t];
        if (!naechster) break;
        timer.delete(naechster[0]);
        uhr = Math.max(uhr, naechster[1].bis);
        naechster[1].fn();
      }
      uhr = ziel;
      await flush(); await flush();
    },
  };
}

const eintrag = (id, extra = {}) => ({
  id, hall: "Halle A", art: "court_call", lang: "de", rate: 0.8,
  segmente: ["Spiel " + id], audio: { mime: "audio/mpeg", bytes: 10 }, ...extra,
});
const feedVon = (eintraege, extra = {}) => ({
  aktiv: true, hostAudio: true, halls: ["Halle A", "Halle B"], belegung: [], eintraege, ...extra,
});
const audioAbrufe = (t) => t.abrufe.filter((u) => u.includes("/ansage/audio/"));
const feedAbrufe = (t) => t.abrufe.filter((u) => u.includes("/ansage/feed"));

// ── Bestandsaufnahme, Präsenz, strikte Reihenfolge ─────────────────────
{
  const t = welt();
  const k = ansageKern({ base: "/", halle: "Halle A", halleFest: true, welt: t.w });
  t.feed(feedVon([eintrag(1)]));
  t.feed(feedVon([eintrag(1), eintrag(2), eintrag(3)]));
  k.starten();
  await t.lauf(10);
  ok("erster Abruf meldet noch keine Präsenz", feedAbrufe(t)[0].includes("hoert=1"), false);
  ok("erster Abruf fragt mit der festen Halle erst nach dem ersten Stand", feedAbrufe(t)[0].includes("halle=&"), true);
  ok("Bestandsaufnahme: nichts gespielt", audioAbrufe(t), []);
  await t.lauf(ANSAGE_TAKT_MS);
  ok("zweiter Abruf: Halle + Präsenz (Ton läuft ohne Tippen)", feedAbrufe(t)[1], "/ansage/feed?halle=Halle%20A&since=1&hoert=1");
  ok("strikt nacheinander: erst nur Eintrag 2", audioAbrufe(t), ["/ansage/audio/2"]);
  await t.lauf(8000);
  ok("danach Eintrag 3", audioAbrufe(t), ["/ansage/audio/2", "/ansage/audio/3"]);
  ok("letzte Ansage gemerkt", k.zustand.letzte && k.zustand.letzte.text, "Spiel 3");
}

// ── Kriterium 17: ohne Autoplay-Freigabe keine Präsenz, nichts gespielt ─
{
  const t = welt({ tonLaeuft: false });
  const k = ansageKern({ base: "/", halle: "Halle A", halleFest: true, welt: t.w });
  t.feed(feedVon([]));
  t.feed(feedVon([eintrag(5)]));
  t.feed(feedVon([eintrag(5)]));
  k.starten();
  await t.lauf(2 * ANSAGE_TAKT_MS + 10);
  ok("ohne Ton nie hoert=1", feedAbrufe(t).some((u) => u.includes("hoert=1")), false);
  ok("ohne Ton nichts abgespielt", audioAbrufe(t), []);
  ok("Hinweis nennt die Autoplay-Freigabe", monitorHinweis({
    lan: true, verbunden: true, feed: k.zustand.feed, tonFrei: k.tonPruefen(),
    halleOk: k.halleOk(), halle: k.zustand.halle, stimme: k.stimmeArt(),
  }).includes("Autoplay"), true);
  k.freischalten();
  await t.lauf(10);
  ok("eine Geste gibt den Ton frei", k.tonPruefen(), true);
}

// ── Fest vorgegebene Halle, die es nicht gibt ───────────────────────────
{
  const t = welt();
  const k = ansageKern({ base: "/", halle: "Halle C", halleFest: true, welt: t.w });
  t.feed(feedVon([]));
  t.feed(feedVon([eintrag(9)]));
  k.starten();
  await t.lauf(ANSAGE_TAKT_MS + 10);
  ok("feste Halle bleibt stehen", k.zustand.halle, "Halle C");
  ok("… gilt aber nicht", k.halleOk(), false);
  ok("… keine Präsenz", feedAbrufe(t).some((u) => u.includes("hoert=1")), false);
  ok("… nichts gespielt", audioAbrufe(t), []);
  ok("Hinweis nennt die Halle", monitorHinweis({
    lan: true, verbunden: true, feed: k.zustand.feed, tonFrei: true, halleOk: false, halle: "Halle C",
  }).includes("„Halle C“"), true);
}
{
  const t = welt();
  const k = ansageKern({ base: "/", halle: "halle b", halleFest: true, welt: t.w });
  t.feed(feedVon([]));
  k.starten();
  await t.lauf(10);
  ok("feste Halle übernimmt die Schreibweise des Turniers", k.zustand.halle, "Halle B");
}
{
  const t = welt();
  const k = ansageKern({ base: "/", halle: "Halle C", welt: t.w });
  t.feed(feedVon([]));
  k.starten();
  await t.lauf(10);
  ok("/ansage (nicht fest): unbekannte Halle wird verworfen", k.zustand.halle, "");
}
{
  // Turnier mit einer Halle: Liste leer, Wahl egal.
  const t = welt();
  const k = ansageKern({ base: "/", halle: "Halle A", halleFest: true, welt: t.w });
  t.feed(feedVon([], { halls: [] }));
  t.feed(feedVon([eintrag(4)], { halls: [] }));
  k.starten();
  await t.lauf(ANSAGE_TAKT_MS + 10);
  ok("eine Halle: fragt ohne Halle, mit Präsenz", feedAbrufe(t)[1], "/ansage/feed?halle=&since=0&hoert=1");
  ok("eine Halle: spielt", audioAbrufe(t), ["/ansage/audio/4"]);
}

// ── Belegung: anderes Spiel auf dem Feld → schweigen ────────────────────
{
  const t = welt();
  const k = ansageKern({ base: "/", halle: "Halle A", halleFest: true, welt: t.w });
  t.feed(feedVon([]));
  t.feed(feedVon([eintrag(2, { courtId: 1, matchId: 7 })], { belegung: [{ courtId: 1, matchId: 8 }] }));
  k.starten();
  await t.lauf(ANSAGE_TAKT_MS + 10);
  ok("anderes Spiel auf dem Feld → nichts", audioAbrufe(t), []);
}

// ── Audio-Weg: Fehler und Hänger fallen auf die Gerätestimme zurück ────
for (const [art, warten, name] of [["404", 10, "Audio 404"], ["haengt", AUDIO_LADE_FRIST_MS + 10, "Audio hängt (Frist)"]]) {
  const t = welt({ stimmen: [{ lang: "de-DE" }] });
  t.audioAntwort.art = art;
  const k = ansageKern({ base: "/", halle: "Halle A", halleFest: true, welt: t.w });
  t.feed(feedVon([]));
  t.feed(feedVon([eintrag(2, { gong: "match" })]));
  k.starten();
  await t.lauf(ANSAGE_TAKT_MS + 2000 + warten);
  ok(`${name} → Gerätestimme spricht die Segmente`, t.gesprochen, ["Spiel 2"]);
}

// ── Feed-Abruf: Frist, kein Parallel-Abruf, 404 bremst ──────────────────
{
  const t = welt();
  const k = ansageKern({ base: "/", halle: "", welt: t.w });
  k.starten(); // keine Antwort vorbereitet → Abruf hängt
  await t.lauf(ANSAGE_FEED_FRIST_MS - 10);
  ok("hängender Abruf: kein zweiter parallel", feedAbrufe(t).length, 1);
  ok("… solange gilt die Seite noch nicht als getrennt", k.zustand.verbunden, false);
  await t.lauf(ANSAGE_TAKT_MS + 20);
  ok("nach der Frist fragt sie wieder", feedAbrufe(t).length, 2);
}
{
  const t = welt();
  const k = ansageKern({ base: "/", halle: "", welt: t.w });
  t.status(404);
  k.starten();
  await t.lauf(10 * ANSAGE_TAKT_MS);
  ok("404: kein Abruf im Takt", feedAbrufe(t).length, 1);
  ok("404: Hinweis statt Fehlerschleife", monitorHinweis({ lan: true, fehlt: !!k.zustand.fehltSeit }).includes("keine Ansagen"), true);
  t.feed(feedVon([]));
  await t.lauf(ANSAGE_FEHLT_PAUSE_MS - 10 * ANSAGE_TAKT_MS - 100);
  ok("404: auch kurz vor einer Minute noch Ruhe", feedAbrufe(t).length, 1);
  await t.lauf(1000);
  ok("404: nach einer Minute ein neuer Versuch", feedAbrufe(t).length, 2);
  ok("… und danach wieder verbunden", k.zustand.verbunden && !k.zustand.fehltSeit, true);
}

// ── Monitor-Modus: Adresse, LAN/Cloud, Hinweise ─────────────────────────
ok("ohne Parameter kein Modus", ansageModusAusQuery("?device=abc"), null);
ok("?ansage=Halle%20A", ansageModusAusQuery("?device=abc&ansage=Halle%20A"), { halle: "Halle A" });
ok("?ansage leer (eine Halle)", ansageModusAusQuery("?ansage"), { halle: "" });
ok("LAN-Basis", ansageImLan("/"), true);
ok("Cloud-Basis", ansageImLan("/bts-relay/abc/"), false);
ok("Cloud: Hinweis ohne Abruf", monitorHinweis({ lan: false }).includes("Cloud"), true);
ok("alles bereit: kein Hinweis", monitorHinweis({
  lan: true, verbunden: true, feed: { aktiv: true, hostAudio: true }, tonFrei: true, halleOk: true, halle: "A", stimme: "host",
}), "");
ok("Feed aus: Grund", monitorHinweis({ lan: true, verbunden: true, feed: { aktiv: false, grund: "schalter_aus" } }).includes("Audio für Ansage-Seiten erzeugen"), true);
ok("ohne Stimme: Hinweis", monitorHinweis({
  lan: true, verbunden: true, feed: { aktiv: true }, tonFrei: true, halleOk: true, stimme: "keine",
}).includes("kann nicht sprechen"), true);

// ── Inline-Kopien: drei Module × vier Seiten ────────────────────────────
{
  const hier = dirname(fileURLToPath(import.meta.url));
  const seiten = ["ansage.html", "monitor.html", "overview.html", "combo.html"];
  for (const modulName of ["ansageSeite.mjs", "gongKlang.mjs", "ansageKern.mjs"]) {
    const modul = readFileSync(join(hier, "..", "src", "io", modulName), "utf8");
    const erwartet = modul.split("\n\n").slice(1).join("\n\n").replace(/^export /gm, "").trim();
    const marke = modulName.replace(/\./g, "\\.");
    const muster = new RegExp(`/\\* <${marke}> \\*/\\n([\\s\\S]*?)\\n/\\* </${marke}> \\*/`);
    for (const seite of seiten) {
      const html = readFileSync(join(hier, "..", "src-tauri", "assets", seite), "utf8");
      const m = html.match(muster);
      ok(`${seite} trägt ${modulName} wortgleich`, !!m && m[1].trim() === erwartet, true);
    }
  }
  for (const seite of ["monitor.html", "overview.html", "combo.html"]) {
    const html = readFileSync(join(hier, "..", "src-tauri", "assets", seite), "utf8");
    ok(`${seite} startet den Ansage-Modus`, html.includes("ansageMonitorStarten(ANSAGE_BASIS, window, document);"), true);
  }
}

if (failures) {
  console.error(`${failures} Test(s) fehlgeschlagen`);
  process.exit(1);
}
console.log("Alle Ansage-Kern-Tests grün.");
