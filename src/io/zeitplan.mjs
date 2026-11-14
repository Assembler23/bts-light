// Zeitplan im TL-Web (Spec `tl-zeitplan`, ADR 0094): Texte zur geplanten Zeit
// eines Spiels.
//
// Kanonische Fassung; `src-tauri/assets/tl.html` trägt eine Inline-Kopie
// (die Seite wird ohne Build-Schritt ausgeliefert).
// `scripts/test-zeitplan.mjs` prüft beide auf gleiches Verhalten.

/**
 * Planzeit aus BTP (`YYYYMMDDHHMM`, Wandzeit des Turnier-PCs) als Text:
 * „10:20", an einem anderen Tag als `heute` (`YYYYMMDD` vom Turnier-PC) mit
 * Wochentag: „So 09:00".
 *
 * Gelesen werden nur die Ziffern — nie ein `Date` in der Zone des Geräts.
 * Ein Turnierleitungs-Gerät mit anderer Zeitzone zeigt deshalb dieselbe
 * Uhrzeit wie der Turnier-PC. Der Wochentag kommt aus einer UTC-Rechnung
 * über Jahr/Monat/Tag und hängt ebenfalls an keiner Zone. Kennt der Host
 * `heute` nicht (0, fehlt), steht nur die Uhrzeit. Unsinn ergibt "".
 */
export function planZeitText(plannedTime, heute) {
  const n = Number(plannedTime);
  if (!Number.isFinite(n) || n < 100000000000 || n > 999912312359) return "";
  const minute = n % 100;
  const stunde = Math.floor(n / 100) % 100;
  const datum = Math.floor(n / 10000);
  const tag = datum % 100;
  const monat = Math.floor(datum / 100) % 100;
  const jahr = Math.floor(datum / 10000);
  if (minute > 59 || stunde > 23 || monat < 1 || monat > 12 || tag < 1 || tag > 31) return "";
  const uhr = `${String(stunde).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
  const h = Number(heute);
  if (!Number.isFinite(h) || h <= 0 || h === datum) return uhr;
  const wochentag = ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"][
    new Date(Date.UTC(jahr, monat - 1, tag)).getUTCDay()
  ];
  return `${wochentag} ${uhr}`;
}

/**
 * Abstand des tatsächlichen Starts zur Planzeit: „+18 min", „−4 min",
 * „±0 min". Fehlt die Messung (Walkover, kein Stempel), "".
 */
export function startAbweichungText(mins) {
  if (mins === null || mins === undefined || mins === "") return "";
  const m = Math.trunc(Number(mins));
  if (!Number.isFinite(m)) return "";
  if (m === 0) return "±0 min";
  return `${m > 0 ? "+" : "−"}${Math.abs(m)} min`;
}
