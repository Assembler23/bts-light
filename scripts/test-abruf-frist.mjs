// Testet die Abruf-Frist der Anzeige-Seiten (src/io/abrufFrist.mjs) — das
// echte Modul, dessen Inline-Kopie monitor/overview/tafel/combo/winners/
// preparation/lobby/ad/tl tragen.
//
// Hintergrund (Turnier 05./06.09.2026, LAN, sechs Pi-Monitore): Die Anzeigen
// froren phasenweise ein, ohne Offline-Blende, während Tablets und TL-Web am
// selben Server sauber liefen. Der Stand-Abruf hatte keinen Timeout: Ging
// eine Antwort im WLAN-Roaming verloren, blieb `fetching` gesetzt, der
// In-Flight-Schutz ließ keinen weiteren Poll zu, und jeder Nudge landete nur
// noch im Merker. Erst die TCP-Sendewiederholung des Pi löste den Knoten —
// nach Minuten.
//
// Was hier geprüft wird: Ein Abruf endet SPÄTESTENS nach den Fristen als
// Fehler — egal ob nie Kopfzeilen kamen oder der Rumpf hängt —, ein
// rechtzeitiger Abruf bleibt in jeder Hinsicht unverändert, ein langsamer
// aber lebender Rumpf bekommt sein eigenes Budget, und eine verspätete
// Antwort läuft nicht mehr in die Seite.
import {
  abrufMitFrist,
  KOPF_FRIST_MS,
  RUMPF_FRIST_MS,
} from "../src/io/abrufFrist.mjs";
import { HERZSCHLAG_STILL_MS } from "../src/io/pushHealth.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got);
  const w = JSON.stringify(want);
  if (g !== w) {
    console.error(`✗ ${name}: erwartet ${w}, war ${g}`);
    failures++;
  } else {
    console.log(`✓ ${name}`);
  }
}

const schlaf = (ms) => new Promise((r) => setTimeout(r, ms));

/** Nachbau von fetch: liefert nach `kopfNachMs` eine Antwort, deren
 *  `json()` nach `rumpfNachMs` auflöst. `Infinity` = kommt nie. Bricht das
 *  Signal ab, verwerfen beide Stufen wie der Browser mit `AbortError`.
 *  `ohneSignal` = Browser ohne AbortController: das Signal wird ignoriert. */
function fakeFetch(kopfNachMs, rumpfNachMs, aufzeichnung, ohneSignal) {
  return (url, opt) => {
    aufzeichnung.url = url;
    aufzeichnung.opt = opt;
    const signal = ohneSignal ? null : opt && opt.signal;
    const abbruch = new Promise((_, reject) => {
      if (!signal) return;
      const fehler = () => reject(Object.assign(new Error("abgebrochen"), { name: "AbortError" }));
      if (signal.aborted) fehler();
      else signal.addEventListener("abort", fehler);
    });
    const nach = (ms, wert) =>
      ms === Infinity ? new Promise(() => {}) : schlaf(ms).then(() => wert);
    const antwort = {
      ok: true,
      status: 200,
      json: () => Promise.race([nach(rumpfNachMs, { stand: 42 }), abbruch]),
    };
    return Promise.race([nach(kopfNachMs, antwort), abbruch]);
  };
}

const lesen = (r) => r.json();

// ── Rechtzeitige Antwort: alles unverändert ──────────────────────────────
{
  const auf = {};
  const wert = await abrufMitFrist(fakeFetch(5, 5, auf), "/health", { cache: "no-store", headers: { "If-None-Match": "x" } }, lesen, 100, 100);
  ok("rechtzeitig: Ergebnis des Lesers kommt durch", wert, { stand: 42 });
  ok("rechtzeitig: URL unverändert", auf.url, "/health");
  ok("rechtzeitig: cache-Option bleibt", auf.opt.cache, "no-store");
  ok("rechtzeitig: Kopfzeilen bleiben", auf.opt.headers, { "If-None-Match": "x" });
  ok("rechtzeitig: Signal wird mitgegeben", typeof auf.opt.signal === "object" && auf.opt.signal !== null, true);
  ok("rechtzeitig: Signal nicht abgebrochen", auf.opt.signal.aborted, false);
  // Kein Timer darf nachträglich feuern — weder der für die Kopfzeilen noch
  // der für den Rumpf.
  await schlaf(250);
  ok("rechtzeitig: auch nach Ablauf beider Fristen kein Abbruch", auf.opt.signal.aborted, false);
}

// ── Die ursprünglichen Optionen bleiben unberührt ─────────────────────────
{
  const opt = { cache: "no-store" };
  await abrufMitFrist(fakeFetch(1, 1, {}), "/x", opt, lesen, 100, 100);
  ok("Optionen des Aufrufers bekommen kein Signal angehängt", "signal" in opt, false);
}

// ── Ohne Optionen ─────────────────────────────────────────────────────────
{
  const auf = {};
  const wert = await abrufMitFrist(fakeFetch(1, 1, auf), "/x", undefined, lesen, 100, 100);
  ok("ohne Optionen: läuft, Signal gesetzt", wert.stand === 42 && !!auf.opt.signal, true);
}

// ── Kopfzeilen kommen nie: Kopf-Frist beendet den Abruf als Fehler ────────
{
  const auf = {};
  const t0 = Date.now();
  let fehler = null;
  try { await abrufMitFrist(fakeFetch(Infinity, 0, auf), "/health", {}, lesen, 60, 1000); }
  catch (e) { fehler = e; }
  ok("hängende Kopfzeilen: verwirft", fehler !== null, true);
  ok("hängende Kopfzeilen: Meldung nennt die Kopfzeilen", /Kopfzeilen 60 ms/.test(fehler && fehler.message), true);
  ok("hängende Kopfzeilen: Signal abgebrochen", auf.opt.signal.aborted, true);
  const dauer = Date.now() - t0;
  ok("hängende Kopfzeilen: binnen Kopf-Frist, nicht erst nach der Rumpf-Frist", dauer >= 50 && dauer < 600, true);
}

// ── Kopfzeilen da, Rumpf hängt: Rumpf-Frist deckt das Lesen ab ────────────
{
  const auf = {};
  const t0 = Date.now();
  let fehler = null;
  try { await abrufMitFrist(fakeFetch(5, Infinity, auf), "/health", {}, lesen, 60, 120); }
  catch (e) { fehler = e; }
  ok("hängender Rumpf: verwirft", fehler !== null, true);
  ok("hängender Rumpf: Meldung nennt den Rumpf", /Rumpf 120 ms/.test(fehler && fehler.message), true);
  ok("hängender Rumpf: Signal abgebrochen", auf.opt.signal.aborted, true);
  const dauer = Date.now() - t0;
  ok("hängender Rumpf: Rumpf-Budget zählt ab den Kopfzeilen, nicht ab Start", dauer >= 110, true);
}

// ── Langsamer, aber lebender Rumpf: kommt durch ───────────────────────────
// Kopfzeilen nach 5 ms, Rumpf braucht 150 ms — länger als die Kopf-Frist,
// aber innerhalb des Rumpf-Budgets. Vor den zwei Budgets wäre das ein
// endloser Fehlschlag auf einer langsamen, lebenden Leitung gewesen.
{
  const auf = {};
  const wert = await abrufMitFrist(fakeFetch(5, 150, auf), "/health", {}, lesen, 60, 400);
  ok("langsamer Rumpf innerhalb des Rumpf-Budgets: Ergebnis kommt", wert, { stand: 42 });
  ok("langsamer Rumpf: kein Abbruch", auf.opt.signal.aborted, false);
}

// ── Browser ohne AbortController: Wettlauf allein beendet den Abruf ───────
{
  const auf = {};
  let fehler = null;
  try { await abrufMitFrist(fakeFetch(Infinity, 0, auf, true), "/health", {}, lesen, 60, 60); }
  catch (e) { fehler = e; }
  ok("ohne Abbruch-Möglichkeit: Frist verwirft trotzdem", fehler !== null, true);
}

// ── Verspätete Antwort nach der Frist: der Leser läuft NICHT mehr ─────────
// Ohne AbortController kommt die Antwort irgendwann doch. Liefe der Leser
// dann noch, räumte er die Blende ab oder gäbe den In-Flight-Schutz für
// einen fremden, längst laufenden Abruf frei.
{
  const auf = {};
  let leserLief = 0;
  let fehler = null;
  try {
    await abrufMitFrist(fakeFetch(80, 0, auf, true), "/health", {}, (r) => { leserLief++; return r.json(); }, 30, 1000);
  } catch (e) { fehler = e; }
  ok("verspätet: Frist hat verworfen", fehler !== null, true);
  await schlaf(120);
  ok("verspätet: Leser lief nicht", leserLief, 0);
}

// ── Eigener Fehler des Abrufs: durchgereicht, Timer geräumt ───────────────
{
  const auf = {};
  const kaputt = (url, opt) => { auf.opt = opt; return Promise.reject(new Error("HTTP 503")); };
  let fehler = null;
  try { await abrufMitFrist(kaputt, "/health", {}, lesen, 60, 60); }
  catch (e) { fehler = e; }
  ok("eigener Fehler: Meldung durchgereicht", fehler && fehler.message, "HTTP 503");
  await schlaf(100);
  ok("eigener Fehler: Timer feuert nicht nach", auf.opt.signal.aborted, false);
}

// ── Fehler im Leser (z. B. `throw new Error('HTTP 404')`): durchgereicht ──
{
  let fehler = null;
  try {
    await abrufMitFrist(fakeFetch(1, 1, {}), "/health", {}, () => { throw new Error("HTTP 404"); }, 60, 60);
  } catch (e) { fehler = e; }
  ok("Fehler im Leser: durchgereicht", fehler && fehler.message, "HTTP 404");
}

// ── Ohne Leser: die rohe Antwort kommt zurück ─────────────────────────────
{
  const wert = await abrufMitFrist(fakeFetch(1, 1, {}), "/health", {}, null, 60, 60);
  ok("ohne Leser: rohe Antwort", wert.status, 200);
}

// ── Ohne Frist-Angaben gelten die Standards ───────────────────────────────
{
  const auf = {};
  await abrufMitFrist(fakeFetch(1, 1, auf), "/health", {}, lesen);
  ok("Standard-Fristen: läuft (Signal gesetzt, nicht abgebrochen)", auf.opt.signal.aborted, false);
}

// ── Die Fristen selbst ────────────────────────────────────────────────────
// Beide zusammen müssen unter der Herzschlag-Schwelle liegen: Der WebSocket
// gibt einen toten Kanal nach 25 s auf, der Datenweg darf nicht länger
// zögern als der Anstoß-Weg. Die Kopf-Frist muss über jeder ehrlichen
// Antwortzeit liegen (am Relay unter Last wäre eine Sekunde zu knapp), das
// Rumpf-Budget deutlich darüber — 16 KB Übersicht über einen LTE-Hotspot am
// Rand sind langsam, aber lebendig.
ok("Kopf- und Rumpf-Frist zusammen unter der Herzschlag-Schwelle", KOPF_FRIST_MS + RUMPF_FRIST_MS < HERZSCHLAG_STILL_MS, true);
ok("Kopf-Frist ist kein Schnellschuss", KOPF_FRIST_MS >= 3000, true);
ok("Rumpf-Budget ist großzügiger als die Kopf-Frist", RUMPF_FRIST_MS > KOPF_FRIST_MS, true);

// ── Drift-Wächter der Inline-Kopien ───────────────────────────────────────
// Neun Seiten tragen die Kopie von Hand. Landet eine Änderung im Modul und in
// sieben Seiten, blieben zwei beim alten Verhalten — und CI wäre grün.
// Deshalb: alle Kopien byte-gleich (nach Einrückung und Anführungszeichen),
// jede Seite ruft den Helfer auch auf, und die Budgets der Kopien sind die
// des Moduls. Gleiches Muster wie `scripts/test-entschlackung-liste.mjs`.
{
  const { readFileSync } = await import("node:fs");
  const { fileURLToPath } = await import("node:url");
  const { dirname, join } = await import("node:path");
  const assets = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri", "assets");
  const seiten = ["monitor.html", "overview.html", "tafel.html", "combo.html", "winners.html",
    "preparation.html", "lobby.html", "ad.html", "tl.html"];
  // Einrückung der ersten Zeile von allen Zeilen abziehen (tl.html ist ein
  // Modul-Skript ohne umschließende Funktion, dort liegt alles zwei Stufen
  // weiter links), Anführungszeichen vereinheitlichen.
  const normal = (txt) => {
    const tiefe = txt.match(/^ */)[0].length;
    return txt.split("\n")
      .map((z) => z.slice(Math.min(tiefe, z.match(/^ */)[0].length)))
      .join("\n")
      .replace(/'/g, '"');
  };
  let referenz = null;
  for (const seite of seiten) {
    const html = readFileSync(join(assets, seite), "utf8");
    const m = html.match(/ *function abrufMitFrist\(url, opt, lesen\) \{[\s\S]*?\n {0,2}\}\n/);
    ok(`${seite}: trägt die Kopie`, !!m, true);
    if (!m) continue;
    const koerper = normal(m[0]);
    if (referenz === null) referenz = koerper;
    ok(`${seite}: Kopie gleicht der in ${seiten[0]}`, koerper === referenz, true);
    const b = html.match(/var KOPF_FRIST_MS = (\d+), RUMPF_FRIST_MS = (\d+);/);
    ok(`${seite}: Budgets wie im Modul`, b && [Number(b[1]), Number(b[2])], [KOPF_FRIST_MS, RUMPF_FRIST_MS]);
    const aufrufe = (html.match(/(?<!function )abrufMitFrist\(/g) || []).length;
    ok(`${seite}: ruft den Helfer auf`, aufrufe >= 1, true);
  }
}

if (failures) {
  console.error(`${failures} Test(s) fehlgeschlagen`);
  process.exit(1);
}
console.log("Alle Tests bestanden");
