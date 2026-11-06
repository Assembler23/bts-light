// „Zuletzt gespielt" in der Turnierleitungs-Liste (Spec `tl-zuletzt-gespielt`):
// wie lange das letzte Spiel einer Seite her ist, als kurzer Text.
//
// Kanonische Fassung; `src-tauri/assets/tl.html` trägt eine Inline-Kopie
// (die Seite wird ohne Build-Schritt ausgeliefert).
// `scripts/test-zuletzt-gespielt.mjs` prüft beide auf gleiches Verhalten.

/**
 * Text zur Marke: unter einer Minute „gerade eben", sonst „vor N min", ab
 * einer Stunde „vor 1 h 05". Minuten werden abgerundet — „vor 1 min" erst
 * nach voller Minute. Ein Zeitstempel in der Zukunft (Uhrversatz) zählt als
 * „gerade eben", nie als negative Zeit.
 */
export function zuletztText(sinceMs, nowMs) {
  const min = Math.max(0, Math.floor((nowMs - sinceMs) / 60000));
  if (!Number.isFinite(min) || min < 1) return "gerade eben";
  if (min < 60) return `vor ${min} min`;
  const h = Math.floor(min / 60);
  return `vor ${h} h ${String(min % 60).padStart(2, "0")}`;
}
