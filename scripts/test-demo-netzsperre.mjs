// Demofeld (Spec tablet-demofeld): Die Netzsperre `demoNetzSperren` aus
// `src-tauri/assets/tablet.html` ist die Zusage „im Übungsspiel geht nichts
// raus". Dieser Test zieht die ECHTE Funktion aus der Seite und prüft sie
// gegen ein nachgebautes `window` — kein Abgleich mit einer Kopie.
import fs from 'node:fs';
import assert from 'node:assert/strict';

const html = fs.readFileSync(new URL('../src-tauri/assets/tablet.html', import.meta.url), 'utf8');
const start = html.indexOf('function demoNetzSperren() {');
assert.ok(start >= 0, 'demoNetzSperren nicht gefunden');
// Ende: die erste Zeile, die nur aus „}" besteht (Funktion auf oberster Ebene).
const ende = html.indexOf('\n}\n', start);
const quelle = html.slice(start, ende + 2);
assert.ok(html.includes("const DEMO = '__DEMO__' === '1';"), 'Demo-Schalter umbenannt?');
assert.ok(/if \(DEMO\) demoNetzSperren\(\);/.test(html), 'Sperre wird nicht mehr aufgerufen');
assert.ok(/function connect\(\) \{\s*\/\/[^\n]*\n[^\n]*\n\s*if \(DEMO\) \{/.test(html), 'connect() ohne Demo-Wächter am Anfang');

const echteAufrufe = [];
const window = {
  fetch: (eingabe, opt) => { echteAufrufe.push([String(eingabe), (opt && opt.method) || 'GET']); return Promise.resolve('echt'); },
  WebSocket: function () {},
};
const navigator = { sendBeacon: () => { echteAufrufe.push(['beacon']); return true; } };
new Function('window', 'navigator', 'Response', quelle + '\ndemoNetzSperren();')(window, navigator, Response);

// Durch: nur die zwei Lesewege.
assert.equal(await window.fetch('/courts', { cache: 'no-store' }), 'echt');
assert.equal(await window.fetch('/bts-relay/abc/info/ad/state'), 'echt');
assert.equal(echteAufrufe.length, 2);

// Lokal bestätigt: das Ergebnis.
const r = await window.fetch('/result', { method: 'POST', body: '{}' });
assert.equal(r.ok, true);
assert.deepEqual(await r.json(), { ok: true });

// Abgewiesen: alles andere, auch Lesewege mit Nebenwirkung und POST auf /courts.
for (const [url, method] of [
  ['/health?src=check&court=0', 'GET'],
  ['/tablet-log?court=0', 'POST'],
  ['/meetingpoint/checkin', 'POST'],
  ['/courts', 'POST'],
  ['/result', 'GET'],
  ['/result-x', 'POST'],
]) {
  await assert.rejects(window.fetch(url, { method }), /Demofeld/, `${method} ${url} kam durch`);
}
assert.equal(navigator.sendBeacon('/tablet-log', 'x'), false);
assert.throws(() => new window.WebSocket('ws://x/ws'), /Demofeld/);
assert.equal(window.WebSocket.OPEN, 1);
assert.equal(window.WebSocket.CLOSED, 3);
assert.equal(echteAufrufe.length, 2, 'ein gesperrter Weg erreichte das echte Netz');

console.log('✓ Demofeld-Netzsperre: alle Prüfungen bestanden');
