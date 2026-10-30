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

// ── Verlauf + Wiederholen (Erweiterung 30.10.2026) ──────────────────────
{
  const t = welt();
  const k = ansageKern({ base: "/", halle: "Halle A", halleFest: true, welt: t.w, verlauf: true });
  const sieben = [1, 2, 3, 4, 5, 6, 7].map((i) => eintrag(i));
  t.feed(feedVon([]));
  t.feed(feedVon(sieben));
  t.feed(feedVon(sieben.concat([eintrag(8, { courtId: 3, matchId: 99 })])));
  k.starten();
  await t.lauf(ANSAGE_TAKT_MS + 10);
  ok("Verlauf: die gerade erklingende steht schon vorn", k.zustand.verlauf.map((v) => v.id), [1]);
  ok("… mit ihrem Text", k.zustand.verlauf[0].text, "Spiel 1");
  await t.lauf(30000);
  ok("Verlauf: Deckel 5, neueste vorn", k.zustand.verlauf.map((v) => v.id), [7, 6, 5, 4, 3]);
  ok("übersprungene (falsches Spiel) kommt nicht in den Verlauf", k.zustand.verlauf.some((v) => v.id === 8), false);
  ok("Audio der fünf liegt im Speicher", k.zustand.verlauf.every((v) => !!v.puffer), true);
  ok("letzte bleibt gesetzt", k.zustand.letzte.text, "Spiel 7");

  // Nach Ablauf des Server-Audios: ↻ spielt das gemerkte Audio, ohne Abruf.
  t.audioAntwort.art = "404";
  const audioVorher = audioAbrufe(t).length;
  const gespieltVorher = t.gespielt.length;
  await t.lauf(120000);
  ok("↻ auf eine bekannte Ansage", k.wiederholen(4), true);
  await t.lauf(10000);
  ok("↻ spielt das gemerkte Audio", t.gespielt.length, gespieltVorher + 1);
  ok("↻ holt nichts vom Turnier-PC", audioAbrufe(t).length, audioVorher);
  ok("↻ ist kein neuer Verlaufseintrag", k.zustand.verlauf.map((v) => v.id), [7, 6, 5, 4, 3]);
  ok("↻ setzt letzte nicht um", k.zustand.letzte.text, "Spiel 7");
  ok("↻ auf eine unbekannte Ansage", k.wiederholen(99), false);

  // Dreifach-Tipp stapelt nicht; Hallenwechsel behält die Wiederholung.
  const vorTipp = t.gespielt.length;
  k.wiederholen(5); k.wiederholen(6); k.wiederholen(6); k.wiederholen(6);
  ok("Mehrfach-Tipp: eine Wiederholung je Ansage in der Schlange",
    k.zustand.warteschlange.map((s) => s.e.id), [6]);
  k.halleWaehlen("Halle B");
  ok("Hallenwechsel behält angeforderte Wiederholungen", k.zustand.warteschlange.map((s) => s.e.id), [6]);
  await t.lauf(10000);
  ok("… und spielt beide genau einmal", t.gespielt.length, vorTipp + 2);
  ok("Verlauf bleibt nach Hallenwechsel", k.zustand.verlauf.length, 5);
}
{
  // ↻ bei angehaltenem Ton (Anruf, Bildschirm aus): erst freigeben, dann mit Gong.
  const t = welt();
  const k = ansageKern({ base: "/", halle: "Halle A", halleFest: true, welt: t.w, verlauf: true });
  t.feed(feedVon([]));
  t.feed(feedVon([eintrag(1, { gong: "match" })]));
  k.starten();
  await t.lauf(ANSAGE_TAKT_MS + 10000);
  const ctx = k.zustand.ctx;
  ctx.state = "suspended";
  let resumeFertig = null;
  ctx.resume = function () { return new Promise((r) => { resumeFertig = () => { ctx.state = "running"; r(); }; }); };
  let gongs = 0;
  const osz = ctx.createOscillator.bind(ctx);
  ctx.createOscillator = function () { gongs++; return osz(); };
  const vorher = t.gespielt.length;
  k.wiederholen(1);
  await t.lauf(5000);
  ok("angehaltener Ton: wartet auf die Freigabe", [t.gespielt.length, gongs], [vorher, 0]);
  resumeFertig();
  await t.lauf(10000);
  ok("… dann Gong und Audio", [t.gespielt.length, gongs > 0], [vorher + 1, true]);
}
{
  // Ohne Audio im Speicher (Gerätestimme): ↻ spricht die Segmente — auch wenn
  // inzwischen ein anderes Spiel auf dem Feld steht (bewusst angefordert).
  const t = welt({ stimmen: [{ lang: "de-DE" }] });
  const k = ansageKern({ base: "/", halle: "Halle A", halleFest: true, welt: t.w, verlauf: true });
  const e = eintrag(1, { audio: null, courtId: 3, matchId: 42, segmente: ["Feld 3", "Anna gegen Bea"] });
  t.feed(feedVon([]));
  t.feed(feedVon([e], { belegung: [{ courtId: 3, matchId: 42 }] }));
  k.starten();
  await t.lauf(ANSAGE_TAKT_MS + 10000);
  ok("Gerätestimme: gesprochen", t.gesprochen, ["Feld 3", "Anna gegen Bea"]);
  k.zustand.feed.belegung = [{ courtId: 3, matchId: 77 }];
  await t.lauf(90000);
  k.wiederholen(1);
  await t.lauf(10000);
  ok("↻ spricht erneut, trotz Belegung und Verfall", t.gesprochen.length, 4);
  ok("↻ ohne Audio-Abruf", audioAbrufe(t), []);
}
{
  // ↻ während einer laufenden Ansage: strikt danach, nie darüber.
  const t = welt();
  const k = ansageKern({ base: "/", halle: "Halle A", halleFest: true, welt: t.w, verlauf: true });
  t.feed(feedVon([]));
  t.feed(feedVon([eintrag(1)]));
  t.feed(feedVon([eintrag(1), eintrag(2)]));
  k.starten();
  await t.lauf(ANSAGE_TAKT_MS + 5000);
  await t.lauf(ANSAGE_TAKT_MS - 5000 + 10);
  ok("zweite Ansage läuft", k.zustand.spieltId, 2);
  k.wiederholen(1);
  ok("↻ wartet in der Schlange", [k.zustand.spieltId, k.zustand.warteschlange.length], [2, 1]);
  await t.lauf(10000);
  ok("… und kommt danach", [k.zustand.spieltId, k.zustand.warteschlange.length], [null, 0]);
}
{
  // Monitor-Modus: kein Verlauf, kein Audio im Speicher.
  const t = welt();
  const k = ansageKern({ base: "/", halle: "Halle A", halleFest: true, welt: t.w });
  t.feed(feedVon([]));
  t.feed(feedVon([eintrag(1)]));
  k.starten();
  await t.lauf(ANSAGE_TAKT_MS + 10000);
  ok("ohne verlauf-Option bleibt der Verlauf leer", k.zustand.verlauf, []);
  ok("… und ↻ greift ins Leere", k.wiederholen(1), false);
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
