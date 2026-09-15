// Testet die Akku-Anzeige der Tablet-Seiten (src/io/akku.mjs) — das echte
// Modul, dessen Inline-Kopie tablet.html, lobby.html und anzeige.html tragen.
// Die Quellen-Erkennung wird mit nachgebauten `window`-Objekten geprüft:
// Kiosk-App-Brücke (`window.fully`), Web-Battery-API, keine Quelle.
import {
  AKKU_NIEDRIG, AKKU_KRITISCH, AKKU_TAKT_MS,
  akkuStufe, akkuText, akkuAusFully, akkuAusWebBattery, akkuBeobachten,
} from "../src/io/akku.mjs";

let failures = 0;
function ok(name, got, want) {
  const g = JSON.stringify(got), w = JSON.stringify(want);
  if (g !== w) { console.error(`✗ ${name}: erwartet ${w}, war ${g}`); failures++; }
  else console.log(`✓ ${name}`);
}

// ── Stufen ───────────────────────────────────────────────────────────────
ok("Schwellen 20/10", [AKKU_NIEDRIG, AKKU_KRITISCH], [20, 10]);
ok("Takt 60 s — Akku ändert sich langsam, Abfrage soll selbst kaum kosten", AKKU_TAKT_MS, 60000);
ok("100 % ok", akkuStufe(100), "ok");
ok("20 % ok (Schwelle exklusiv)", akkuStufe(20), "ok");
ok("19 % niedrig", akkuStufe(19), "niedrig");
ok("10 % niedrig", akkuStufe(10), "niedrig");
ok("9 % kritisch", akkuStufe(9), "kritisch");
ok("0 % kritisch", akkuStufe(0), "kritisch");
ok("Laden hebt die Stufe nicht", akkuStufe(5, true), "kritisch");
ok("kein Wert → null", akkuStufe(null), null);
ok("NaN → null", akkuStufe(NaN), null);

// ── Text ─────────────────────────────────────────────────────────────────
ok("Text ohne Laden", akkuText(73, false), "🔋 73 %");
ok("Text beim Laden", akkuText(73, true), "⚡ 73 %");
ok("Text rundet", akkuText(72.6, false), "🔋 73 %");
ok("Text deckelt oben", akkuText(140, false), "🔋 100 %");
ok("Text deckelt unten", akkuText(-3, false), "🔋 0 %");
ok("Text ohne Wert leer", akkuText(null, false), "");
ok("Text bei NaN leer", akkuText(NaN, true), "");

// ── Quellen ──────────────────────────────────────────────────────────────
ok("fully: Prozent + Netz", akkuAusFully({ getBatteryLevel: () => 64, isPlugged: () => true }), { percent: 64, charging: true });
ok("fully: ohne isPlugged → nicht am Netz", akkuAusFully({ getBatteryLevel: () => 64 }), { percent: 64, charging: false });
ok("fully: isPlugged wirft → nicht am Netz", akkuAusFully({ getBatteryLevel: () => 64, isPlugged: () => { throw new Error("x"); } }), { percent: 64, charging: false });
ok("fully: Level als String", akkuAusFully({ getBatteryLevel: () => "64" }), { percent: 64, charging: false });
ok("fully: negatives Level (unbekannt) → null", akkuAusFully({ getBatteryLevel: () => -1 }), null);
ok("fully: getBatteryLevel wirft → null", akkuAusFully({ getBatteryLevel: () => { throw new Error("x"); } }), null);
ok("fully: fehlt → null", akkuAusFully(undefined), null);
ok("web: level 0..1 → Prozent", akkuAusWebBattery({ level: 0.73, charging: false }), { percent: 73, charging: false });
ok("web: charging", akkuAusWebBattery({ level: 1, charging: true }), { percent: 100, charging: true });
ok("web: kaputt → null", akkuAusWebBattery({ level: "x" }), null);
ok("web: fehlt → null", akkuAusWebBattery(null), null);

// ── Beobachten: nachgebautes window ──────────────────────────────────────
function fakeWin(extra) {
  const timer = { intervals: [] };
  return Object.assign({
    setInterval: (fn, ms) => { timer.intervals.push({ fn, ms }); return timer.intervals.length; },
    clearInterval: (id) => { timer.intervals[id - 1] = null; },
    navigator: {},
    _timer: timer,
  }, extra);
}

{
  // Kiosk-App-Brücke: sofort lesen, dann im Takt; Stopp räumt den Timer.
  let level = 80;
  const win = fakeWin({ fully: { getBatteryLevel: () => level, isPlugged: () => false } });
  const gemeldet = [];
  const stop = akkuBeobachten(win, (a) => gemeldet.push(a));
  ok("fully: liest sofort", gemeldet, [{ percent: 80, charging: false }]);
  ok("fully: genau ein Timer im 60-s-Takt", win._timer.intervals.map((t) => t && t.ms), [60000]);
  level = 79; win._timer.intervals[0].fn();
  ok("fully: Takt liefert neuen Wert", gemeldet[1], { percent: 79, charging: false });
  ok("fully: Rückgabe ist Stopp-Funktion", typeof stop, "function");
  stop();
  ok("fully: Stopp räumt den Timer", win._timer.intervals, [null]);
}

{
  // Web-Battery-API: Ereignisse statt Dauer-Abfrage (kein eigener Timer),
  // Start-Wert nach dem Promise.
  const hooks = {};
  const bat = { level: 0.5, charging: true, addEventListener: (ev, fn) => { hooks[ev] = fn; } };
  const win = fakeWin({ navigator: { getBattery: () => Promise.resolve(bat) } });
  const gemeldet = [];
  const stop = akkuBeobachten(win, (a) => gemeldet.push(a));
  ok("web: Rückgabe ist Stopp-Funktion", typeof stop, "function");
  await Promise.resolve(); await Promise.resolve();
  ok("web: Startwert nach dem Promise", gemeldet, [{ percent: 50, charging: true }]);
  ok("web: hört auf levelchange + chargingchange", Object.keys(hooks).sort(), ["chargingchange", "levelchange"]);
  bat.level = 0.49; hooks.levelchange();
  ok("web: Ereignis meldet neuen Wert", gemeldet[1], { percent: 49, charging: true });
  ok("web: kein Poll-Timer", win._timer.intervals, []);
}

{
  // Beide Quellen da → Brücke gewinnt (sie funktioniert auch über http).
  const win = fakeWin({
    fully: { getBatteryLevel: () => 33 },
    navigator: { getBattery: () => { throw new Error("darf nicht gerufen werden"); } },
  });
  const gemeldet = [];
  akkuBeobachten(win, (a) => gemeldet.push(a));
  ok("Brücke hat Vorrang", gemeldet, [{ percent: 33, charging: false }]);
}

{
  // Keine Quelle (iPad, LAN-http ohne Kiosk-App): null, nichts gemeldet.
  const win = fakeWin({});
  const gemeldet = [];
  ok("keine Quelle → null", akkuBeobachten(win, (a) => gemeldet.push(a)), null);
  ok("keine Quelle → nichts gemeldet", gemeldet, []);
}

{
  // getBattery lehnt ab → still, nichts gemeldet.
  const win = fakeWin({ navigator: { getBattery: () => Promise.reject(new Error("nope")) } });
  const gemeldet = [];
  akkuBeobachten(win, (a) => gemeldet.push(a));
  await Promise.resolve(); await Promise.resolve();
  ok("getBattery abgelehnt → nichts gemeldet", gemeldet, []);
}

if (failures > 0) { console.error(`${failures} Fehler`); process.exit(1); }
console.log("alle Akku-Tests grün");
