/** Akkustand am Tablet (Spec-Erweiterung 13.09.2026 in
 *  `docs/tablet.md`, Abschnitt „Akkustand am Tablet").
 *
 *  Die Tablets stecken im Kiosk — die Android-Statusleiste ist weg, und der
 *  Akkustand war nur am Turnier-PC (`TabletPanel`) zu sehen. Jetzt zeigt
 *  jede Tablet-Seite ihn selbst: Kopfzeile beim Zählen, Feldwahl, Anzeige-
 *  Hülle. Zwei Quellen, dieselbe Reihenfolge wie beim Melden an den Host:
 *  1. `window.fully` — die Brücke der Kiosk-App (`FullyBruecke.kt`), die
 *     auch über http funktioniert. Wird im Takt abgefragt (kein Ereignis).
 *  2. Web-Battery-API (`navigator.getBattery`) — nur Android/Chrome im
 *     Secure Context; liefert Ereignisse, darum ohne eigenen Timer.
 *  Ohne Quelle (iPad, LAN-http ohne Kiosk-App) bleibt die Anzeige weg.
 *
 *  Sparsamkeit: ein Brücken-Aufruf je Minute, keine Animationen, kein
 *  Dauer-Rendering — die Anzeige darf selbst nicht am Akku zehren.
 *
 *  Kanonische Fassung. `tablet.html`, `lobby.html` und `anzeige.html`
 *  tragen Inline-Kopien (die Assets durchlaufen keinen Build) — Änderungen
 *  hier und dort gemeinsam.
 */

/** Unter diesem Wert wird die Anzeige orange … */
export const AKKU_NIEDRIG = 20;
/** … und unter diesem rot. Beides exklusiv: 20 % ist noch „ok". */
export const AKKU_KRITISCH = 10;
/** Abfrage-Takt der Kiosk-Brücke. */
export const AKKU_TAKT_MS = 60000;

function prozent(v) {
  // `Number(null)` wäre 0 — „kein Wert" darf aber nicht „leer" heißen.
  if (v === null || v === undefined || v === "") return null;
  const n = Number(v);
  if (!Number.isFinite(n)) return null;
  return Math.max(0, Math.min(100, Math.round(n)));
}

/** @returns {"ok"|"niedrig"|"kritisch"|null} null ohne brauchbaren Wert. */
export function akkuStufe(percent) {
  const p = prozent(percent);
  if (p === null) return null;
  if (p < AKKU_KRITISCH) return "kritisch";
  if (p < AKKU_NIEDRIG) return "niedrig";
  return "ok";
}

/** Anzeigetext, z. B. `🔋 73 %` oder `⚡ 73 %` am Netz; leer ohne Wert. */
export function akkuText(percent, charging) {
  const p = prozent(percent);
  if (p === null) return "";
  return (charging ? "⚡ " : "🔋 ") + p + " %";
}

/**
 * Liest die Kiosk-Brücke. `-1` (Level unbekannt) und Ausnahmen → null;
 * ein fehlendes oder werfendes `isPlugged` gilt als „nicht am Netz".
 * @param {unknown} fully `window.fully`
 * @returns {{percent:number, charging:boolean}|null}
 */
export function akkuAusFully(fully) {
  if (!fully || typeof fully.getBatteryLevel !== "function") return null;
  let lvl;
  try { lvl = Number(fully.getBatteryLevel()); } catch (e) { return null; }
  if (!Number.isFinite(lvl) || lvl < 0) return null;
  let charging = false;
  if (typeof fully.isPlugged === "function") {
    try { charging = !!fully.isPlugged(); } catch (e) { charging = false; }
  }
  return { percent: prozent(lvl), charging };
}

/**
 * Liest ein `BatteryManager`-Objekt der Web-API (`level` 0..1).
 * @returns {{percent:number, charging:boolean}|null}
 */
export function akkuAusWebBattery(bat) {
  if (!bat) return null;
  const p = prozent(Number(bat.level) * 100);
  if (p === null) return null;
  return { percent: p, charging: !!bat.charging };
}

/**
 * Richtet die Quelle ein und meldet jeden Stand an `melde({percent, charging})`.
 * Brücke vor Web-API (die Brücke funktioniert auch über http).
 * @param {object} win `window` (Tests reichen einen Nachbau).
 * @param {(a:{percent:number,charging:boolean})=>void} melde
 * @returns {(()=>void)|null} Stopp-Funktion; null, wenn es keine Quelle gibt.
 */
export function akkuBeobachten(win, melde) {
  const fully = win.fully;
  if (fully && typeof fully.getBatteryLevel === "function") {
    const lies = () => { const a = akkuAusFully(fully); if (a) melde(a); };
    lies();
    const t = win.setInterval(lies, AKKU_TAKT_MS);
    return () => win.clearInterval(t);
  }
  const nav = win.navigator;
  if (nav && typeof nav.getBattery === "function") {
    let aktiv = true;
    let bat = null;
    const lies = () => { if (!aktiv) return; const a = akkuAusWebBattery(bat); if (a) melde(a); };
    let p;
    try { p = nav.getBattery(); } catch (e) { return null; }
    Promise.resolve(p).then((b) => {
      bat = b;
      b.addEventListener("levelchange", lies);
      b.addEventListener("chargingchange", lies);
      lies();
    }).catch(() => { /* nicht verfügbar – Anzeige bleibt stumm */ });
    return () => { aktiv = false; };
  }
  return null;
}
