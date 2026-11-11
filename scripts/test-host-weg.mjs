// „Turnier-PC weg" auf den Anzeige-Seiten (Spec relay-haerten, PR 7).
//
// Der Cloud-Relay antwortet ohne Turnier-PC mit 503 und dem Rumpf
// {"grund":"host_weg"}. Fünf Seiten erkennen das mit derselben kleinen
// Funktion `istHostWeg` (Inline-Kopie je Seite, kein Modul-Import möglich).
// Dieser Test hält die Kopien gleich und prüft das Verhalten der Monitor-
// und der Tafel-Seite im Ganzen: Text, Abdunkeln, kein Log-Upload.
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const SEITEN = ["monitor", "tafel", "overview", "preparation", "combo"];
const lies = (n) => readFileSync(new URL(`../src-tauri/assets/${n}.html`, import.meta.url), "utf8");

function funktion(html, name) {
  const start = html.indexOf(`function ${name}(`);
  assert.ok(start >= 0, `${name} fehlt`);
  let tiefe = 0, i = html.indexOf("{", start);
  for (; i < html.length; i++) {
    if (html[i] === "{") tiefe++;
    else if (html[i] === "}") { tiefe--; if (tiefe === 0) break; }
  }
  return html.slice(start, i + 1);
}

let geprueft = 0;
const ok = (name) => { geprueft++; console.log(`✓ ${name}`); };

// 1. Alle fünf Kopien verhalten sich gleich.
for (const seite of SEITEN) {
  const html = lies(seite);
  const istHostWeg = new Function(`${funktion(html, "istHostWeg")}; return istHostWeg;`)();
  assert.equal(istHostWeg(503, '{"grund":"host_weg"}'), true, seite);
  assert.equal(istHostWeg(503, '{"grund":"anderes"}'), false, seite);
  assert.equal(istHostWeg(503, "Service Unavailable"), false, `${seite}: 503 von nginx`);
  assert.equal(istHostWeg(503, ""), false, seite);
  assert.equal(istHostWeg(502, '{"grund":"host_weg"}'), false, `${seite}: nur 503`);
  assert.equal(istHostWeg(200, '{"grund":"host_weg"}'), false, seite);
  // Der Leser reicht 503 an `wirf503` weiter, bevor der allgemeine Fehler greift.
  const a = html.indexOf("if (r.status === 503) return wirf503(r);");
  assert.ok(a >= 0, `${seite}: 503-Zweig im Leser`);
  ok(`${seite}.html: istHostWeg`);
}

// 2. Monitor und Tafel: Text, Abdunkeln, kein Log-Upload bei „Turnier-PC weg".
for (const seite of ["monitor", "tafel"]) {
  const html = lies(seite);
  const klassen = new Set();
  const logs = [];
  let uploads = 0;
  const els = { offline: { textContent: "", classList: {
    add: (k) => klassen.add("offline." + k), remove: (k) => klassen.delete("offline." + k) } } };
  const document = { body: { classList: {
    toggle: (k, an) => (an ? klassen.add(k) : klassen.delete(k)), remove: (k) => klassen.delete(k) } } };
  const quelle = ["zeigeStoerung", "stoerungVorbei", "abrufFehlgeschlagen", "wiederOnline"]
    .map((n) => funktion(html, n)).join("\n");
  const api = new Function("els", "document", "mlog", "uploadMonitorLog", `
    var wasOffline = false, stoerungHostWeg = false, hinweisOhneHost = false;
    var letzterAbrufOkMs = 0, letzterStandMs = 0;
    ${quelle}
    return { abrufFehlgeschlagen, wiederOnline, stempel: function () { return [letzterAbrufOkMs, letzterStandMs]; },
             kopplung: function (an) { hinweisOhneHost = an; if (an) zeigeStoerung(true); else stoerungVorbei(); } };
  `)(els, document, (stufe, was) => logs.push(was), () => { uploads++; });

  // Turnier-PC weg: eigener Text, abgedunkelt, KEIN Upload — auch nicht bei der Rückkehr.
  api.abrufFehlgeschlagen({ hostWeg: true });
  api.abrufFehlgeschlagen({ hostWeg: true });
  assert.equal(els.offline.textContent, "Keine Verbindung zum Turnier-PC");
  assert.ok(klassen.has("host-weg") && klassen.has("offline.show"));
  assert.deepEqual(logs, ["turnier_pc_weg"], "einmal geloggt, nicht je Abruf");
  assert.equal(uploads, 0);
  // Der Stillstands-Wächter bekommt ein Lebenszeichen (der Relay hat ja
  // geantwortet) — sonst lüde er nach 60 s doch ein Log hoch.
  assert.ok(api.stempel().every((t) => Date.now() - t < 1000), "Wächter-Stempel frisch");
  api.wiederOnline();
  assert.ok(!klassen.has("host-weg") && !klassen.has("offline.show"));
  assert.equal(uploads, 0);

  // Jeder andere Fehler: wie bisher — alter Text, nicht abgedunkelt, Upload bei Wechsel.
  const vorher = api.stempel().join();
  api.abrufFehlgeschlagen(new Error("HTTP 502"));
  assert.equal(api.stempel().join(), vorher, "echte Störung stempelt nicht");
  assert.equal(els.offline.textContent, "Verbindung unterbrochen");
  assert.ok(!klassen.has("host-weg") && klassen.has("offline.show"));
  assert.equal(uploads, 1);
  api.wiederOnline();
  assert.equal(uploads, 2);

  // Kopplungs-Seite ohne Turnier-PC: Der Hinweis übersteht ein „unverändert".
  api.kopplung(true);
  api.wiederOnline();
  assert.ok(klassen.has("offline.show"), "Hinweis bleibt über 304 hinweg");
  api.kopplung(false);
  assert.ok(!klassen.has("offline.show"));

  // Das Abdunkeln ist im Stylesheet verankert.
  assert.match(html, /body\.host-weg #(match-view|tafel-view)[^{]*\{ opacity: \.25; \}/);
  ok(`${seite}.html: Text, Abdunkeln, kein Log-Upload`);
}

// 3. Die drei Banner-Seiten zeigen ihr Banner schon beim ersten solchen 503.
for (const seite of ["overview", "preparation", "combo"]) {
  const html = lies(seite);
  assert.match(html, /fehler && fehler\.hostWeg\)+ (\{\s*)?showOffline\(true\)/, seite);
  assert.match(html, /Keine Verbindung zum Turnier-PC/, seite);
  ok(`${seite}.html: Banner beim ersten 503`);
}

console.log(`OK — ${geprueft} Prüfungen`);
