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

/**
 * Meldung „Turnierleitung gerufen" (Megafon-Knopf am Zähl-Tablet): alle
 * Felder, an denen gerade jemand auf die Turnierleitung wartet. Leerer Text,
 * wenn niemand ruft. Die Reihenfolge ist die der Felder — einen Zeitpunkt
 * des Rufs kennt der Turnier-PC nicht.
 */
export function rufText(courts) {
  const felder = (courts || []).filter((c) => c && c.official_call);
  if (!felder.length) return "";
  return "📣 Turnierleitung ans Feld gerufen: "
    + felder.map((c) => `Feld ${feldMitHalle(c)}`).join(", ");
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
