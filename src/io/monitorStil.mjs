/** Darstellung des Feld-Monitors (Spec `docs/features/monitor-stile-und-stream.md`,
 *  ADR 0068): Allowlist der Stile, welcher Stil wirkt, Kurzname.
 *
 *  Ein Stil kommt nie roh in DOM oder CSS: Alles, was nicht in `STILE` steht,
 *  wird `standard`. Im Geräte-Modus (`MODE = "device"`) gilt allein der Stil
 *  aus dem Zustand (Zuweisung im Panel), auf festen URLs allein `?stil=`.
 *
 *  Kanonische Fassung. `monitor.html` trägt eine Inline-Kopie (die Assets
 *  durchlaufen keinen Build) — Änderungen hier und dort gemeinsam;
 *  `scripts/test-monitor-stil.mjs` vergleicht beide wortgleich.
 */

export var STILE = ["standard", "international", "riesen", "spieler", "hell"];

/** Ein beliebiger Wert als bekannter Stil, sonst `standard`. */
export function stilAusWert(roh) {
  return typeof roh === "string" && STILE.indexOf(roh) >= 0 ? roh : "standard";
}

/** Der Stil, den die Seite zeigt: Gerät → Zustand, feste URL → `?stil=`. */
export function wirksamerStil(modus, stateStil, urlStil) {
  return stilAusWert(modus === "device" ? stateStil : urlStil);
}

/** Feld-Ziel mit Stil fürs Panel. Standard wird weggelassen (bytegleiche
 *  Zuweisung wie vor den Stilen); andere Ziele kennen keinen Stil. */
export function zielMitStil(ziel, stil) {
  if (!ziel || ziel.kind !== "court") return ziel;
  var s = stilAusWert(stil);
  return s === "standard"
    ? { kind: "court", court_id: ziel.court_id }
    : { kind: "court", court_id: ziel.court_id, stil: s };
}

/** Kurzname eines Spielers: Nachname, sonst aus `name` abgeleitet
 *  („Nachname, Vorname" → vor dem Komma, sonst das letzte Wort), sonst
 *  der volle Name. Die Cloud liefert `given`/`family` leer. */
export function kurzname(name, given, family) {
  var f = typeof family === "string" ? family.trim() : "";
  if (f) return f;
  var n = typeof name === "string" ? name.trim() : "";
  var komma = n.indexOf(",");
  var k = komma >= 0 ? n.slice(0, komma).trim() : n.split(/\s+/).pop() || "";
  return k || n;
}
