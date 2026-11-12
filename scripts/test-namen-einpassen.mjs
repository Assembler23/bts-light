// Lange Nachnamen am Court-Monitor: erst kleiner, dann kürzen.
//
// An Fernsehern, an denen der Pi ein 4:3-Bild ausgibt, blieb von einem
// langen Namen bei drei Sätzen nur „Tap…" (Feldfoto 12.11.2026). Die Seite
// sucht jetzt die größte Schrift, bei der der Name passt — höchstens bis
// zur halben Sollgröße. Geprüft wird die echte Funktion aus monitor.html
// an einem Element, dessen Textbreite mit der Schrift mitwächst.
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const html = readFileSync(new URL("../src-tauri/assets/monitor.html", import.meta.url), "utf8");
const start = html.indexOf("var NAME_MIN_ANTEIL");
const ende = html.indexOf("/* Ende Namen einpassen */", start);
assert.ok(start >= 0 && ende > start, "namenGroesse nicht gefunden");
const namenGroesse = new Function(html.slice(start, ende) + "; return namenGroesse;")();

/** Element mit `zeichen` Buchstaben à 0,6 em in einer `breite` px breiten Zeile. */
function element(zeichen, breite, basisPx) {
  const el = { style: { fontSize: "" }, clientWidth: breite };
  Object.defineProperty(el, "scrollWidth", {
    get() {
      const px = el.style.fontSize ? parseFloat(el.style.fontSize) : basisPx;
      return Math.max(breite, Math.ceil(zeichen * px * 0.6));
    },
  });
  return el;
}
const passt = (zeichen, breite, px) => Math.ceil(zeichen * px * 0.6) <= breite;
let n = 0;
const ok = (name) => { n++; console.log(`✓ ${name}`); };

// Passt der Name, bleibt die Sollgröße.
assert.equal(namenGroesse(element(4, 600, 115), 115), 115);
ok("kurzer Name bleibt bei der Sollgröße");

// Zu lang: größte passende Schrift, eine Stufe größer passt nicht mehr.
{
  const g = namenGroesse(element(11, 500, 115), 115);
  assert.ok(g < 115 && g >= 57, `Größe ${g}`);
  assert.ok(passt(11, 500, g) && !passt(11, 500, g + 1), `nicht die größte passende: ${g}`);
  ok(`langer Name schrumpft auf die größte passende Schrift (${g}px)`);
}

// Viel zu lang: nicht unter die halbe Sollgröße — dann kürzt die Ellipsis.
assert.equal(namenGroesse(element(40, 300, 115), 115), 57);
ok("Untergrenze ist die halbe Sollgröße");

// Die Funktion hinterlässt keine Messgröße am Element.
{
  const el = element(11, 500, 115);
  namenGroesse(el, 115);
  assert.equal(el.style.fontSize, "");
  ok("Messgröße wird wieder abgeräumt");
}

// Unsichtbar (anderer Stil, Ansicht verborgen) oder ohne Sollgröße: nichts tun.
assert.equal(namenGroesse({ style: {}, clientWidth: 0, scrollWidth: 0 }, 115), 115);
assert.equal(namenGroesse(element(11, 500, 0), 0), 0);
ok("unsichtbares Element bleibt unberührt");

console.log(`\n${n} Prüfungen bestanden.`);
