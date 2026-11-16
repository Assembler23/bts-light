// Störungsband der Turnierleitungs-Oberfläche: welche Zeilen oben über die
// volle Breite stehen — und in welcher Reihenfolge.
//
// Kanonische Fassung; `src-tauri/assets/tl.html` trägt eine Inline-Kopie
// (die Seite wird ohne Build-Schritt ausgeliefert).
// `scripts/test-stoerungsband.mjs` prüft beide auf gleiches Verhalten.

/**
 * Feldname samt Halle. Die Halle steht im Text, weil das Band den
 * Hallenfilter ausdrücklich ignoriert — ein auf die andere Halle gestelltes
 * Gerät muss trotzdem lesen können, wohin es geht. Bei Ein-Hallen-Turnieren
 * ist `location` leer.
 */
export function feldMitHalle(c) {
  return c.location ? `${c.court} (${c.location})` : `${c.court}`;
}

/** Einleitung der Ruf-Zeile — getrennt, weil das Band hinter jedem Feld
 *  einen Knopf setzt und die Zeile dafür selbst zusammenbaut. */
export const RUF_EINLEITUNG = "📣 Turnierleitung ans Feld gerufen: ";

/**
 * Die Felder, an denen gerade jemand auf die Turnierleitung wartet
 * (Megafon-Knopf am Zähl-Tablet) — je Feld die Kennung für die Rücknahme
 * und die Beschriftung fürs Band. Die Reihenfolge ist die der Felder —
 * einen Zeitpunkt des Rufs kennt der Turnier-PC nicht.
 */
export function rufFelder(courts) {
  return (courts || [])
    .filter((c) => c && c.official_call)
    .map((c) => ({ courtId: c.court_id, name: `Feld ${feldMitHalle(c)}` }));
}

/**
 * Meldung „Turnierleitung gerufen" als eine Zeile Text. Leer, wenn niemand
 * ruft.
 */
export function rufText(courts) {
  const felder = rufFelder(courts);
  if (!felder.length) return "";
  return RUF_EINLEITUNG + felder.map((f) => f.name).join(", ");
}

/**
 * Die Zeilen des Bands. Ohne Verbindung steht NUR die Verbindungsmeldung:
 * Der gezeigte Stand kann alt sein, jede andere Aussage wäre wertlos. Sonst
 * stehen alle Meldungen untereinander — der Ruf zuerst, denn dort wartet
 * gerade jemand. Leere Texte fallen weg.
 */
export function stoerungsZeilen({ offline, ruf, fertig, meetingpoint }) {
  if (offline) return [offline];
  return [ruf, fertig, meetingpoint].filter(Boolean);
}
