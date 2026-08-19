/** Wann darf die Feld-Übersicht eine Karte nur nachbessern statt das ganze
 *  Brett neu zu bauen? (Spec `docs/features/monitor-livestand-push.md`, S5)
 *
 *  Bis hierher warf jeder eintreffende Stand das komplette Board weg und
 *  baute es neu — bei zwanzig Feldern rund siebzig Mal je Sekunde ein voller
 *  DOM-Neuaufbau auf jedem Pi, für eine Änderung von zwei Ziffern.
 *
 *  **Die Regel ist bewusst streng.** Gepatcht wird nur der Normalfall: ein
 *  gezählter Punkt im laufenden Satz. Alles, was Aufbau, Reihenfolge, Farbe
 *  oder Zustand einer Karte berührt, führt zum vollen Neubau. Ein zu
 *  vorsichtiges „nein" kostet einen Neubau, den es vorher ohnehin gab; ein
 *  zu großzügiges „ja" hinterlässt eine Karte, die dauerhaft etwas Falsches
 *  zeigt — und das fällt im Turnier niemandem auf, der es nicht weiß.
 *
 *  Kanonische Fassung. `overview.html` trägt eine Inline-Kopie (die Assets
 *  durchlaufen keinen Build und können keine Module laden) — Änderungen hier
 *  und dort gemeinsam.
 */

/** Steht auf dem Feld ein anzeigbares Spiel? (Gleiche Bedingung wie im
 *  Aufbau der Karte: ohne Match oder ohne Mannschaft ist sie „frei".) */
export function istLive(c) {
  return !!(c && c.match_id && c.match_id > 0 && c.team1 && c.team1.length > 0);
}

/** Wird schon gezählt? Entscheidet mit über die Sichtbarkeit der Aufruf-Uhr. */
export function hatBegonnen(c) {
  if (!c) return false;
  if (c.serving_team && c.serving_team > 0) return true;
  const sets = Array.isArray(c.sets) ? c.sets : [];
  return sets.some((s) => satzWert(s, 0) + satzWert(s, 1) > 0);
}

/** Ein Satzwert, egal ob als Paar `[a,b]` oder als Objekt `{a,b}`. */
export function satzWert(s, idx) {
  if (Array.isArray(s)) return s[idx] | 0;
  if (s && typeof s === "object") return (idx === 0 ? s.a : s.b) | 0;
  return 0;
}

/** Ist die Aufruf-Uhr an dieser Karte sichtbar? */
function uhrSichtbar(c, callTimer) {
  if (!callTimer || !callTimer.enabled) return false;
  if (!c || typeof c.on_court_since_ms !== "number" || c.on_court_since_ms <= 0) return false;
  const pause = !!(c.injury || c.official_call);
  return !pause && !hatBegonnen(c);
}

/**
 * Darf die Karte dieses Felds nachgebessert werden?
 *
 * @param {object} vorher Feld-Stand, wie er gerade gezeigt wird.
 * @param {object} nachher Neu eingetroffener Feld-Stand.
 * @param {boolean} brettGleich Feld-Menge, -Reihenfolge und gezeigte Halle unverändert.
 * @param {object|null} callTimer Aufruf-Timer-Einstellung (für die Sichtbarkeit der Uhr).
 * @returns {boolean}
 */
export function istPatchbar(vorher, nachher, brettGleich, callTimer) {
  if (!brettGleich) return false;
  if (!vorher || !nachher) return false;
  if (vorher.court_id !== nachher.court_id) return false;

  // Beschriftung, Halle und Farbe stehen im Kopf bzw. an der Gruppe — sie
  // gelten für freie Karten genauso wie für belegte, deshalb **vor** dem
  // Kurzschluss weiter unten.
  if ((vorher.court || "") !== (nachher.court || "")) return false;
  if ((vorher.location || "") !== (nachher.location || "")) return false;
  if ((vorher.hall_color || "") !== (nachher.hall_color || "")) return false;

  // Match-Wechsel: andere Namen, andere Runde — die ganze Karte ist neu.
  if ((vorher.match_id || 0) !== (nachher.match_id || 0)) return false;
  // Ein leeres Feld hat gar keine Satz-Zeilen; der Übergang leer↔belegt
  // ändert den Aufbau.
  if (istLive(vorher) !== istLive(nachher)) return false;
  // **Ein durchgehend freies Feld ist patchbar — es gibt nichts zu tun.**
  // Zuerst stand hier ein `return false`, und das machte die ganze Etappe
  // wirkungslos: Die Übersicht listet **alle** Felder, in jeder Halle ist
  // ständig eines zwischen zwei Spielen, und ein einziges „nein" zwingt das
  // ganze Brett in den Neubau (Review-Fund 19.08.2026). Der Aufrufer
  // überspringt freie Felder beim Nachbessern.
  if (!istLive(nachher)) return true;

  // Zustands-Marken sitzen als Klasse an der Karte und als Text im Kopf.
  if (!!vorher.injury !== !!nachher.injury) return false;
  if (!!vorher.official_call !== !!nachher.official_call) return false;

  // Ein neuer Satz fügt je Mannschaft eine Ziffer hinzu.
  const satzZahl = (c) => (Array.isArray(c.sets) ? c.sets.length : 0);
  if (satzZahl(vorher) !== satzZahl(nachher)) return false;

  // Aufruf-Uhr: Der erste gezählte Punkt lässt sie verschwinden.
  if (uhrSichtbar(vorher, callTimer) !== uhrSichtbar(nachher, callTimer)) return false;

  // Runden-/Gruppen-Beschriftung steht nur an einer belegten Karte.
  if ((vorher.match_name || "") !== (nachher.match_name || "")) return false;

  // Namen und Nationen gehören zum Match. Sie sollten sich bei gleicher
  // Match-ID nicht ändern — aber „sollte" ist keine Zusicherung, und eine
  // Karte mit falschem Namen wäre schlimmer als ein Neubau.
  if (!listeGleich(vorher.team1, nachher.team1)) return false;
  if (!listeGleich(vorher.team2, nachher.team2)) return false;
  if (!listeGleich(vorher.team1_nationalities, nachher.team1_nationalities)) return false;
  if (!listeGleich(vorher.team2_nationalities, nachher.team2_nationalities)) return false;

  return true;
}

/** Zwei Namens-/Nationenlisten inhaltlich gleich? */
function listeGleich(a, b) {
  const x = Array.isArray(a) ? a : [];
  const y = Array.isArray(b) ? b : [];
  if (x.length !== y.length) return false;
  return x.every((v, i) => v === y[i]);
}

/**
 * Kennung der **gezeigten** Anordnung: Felder in ihrer Reihenfolge samt
 * Halle, dazu die aktuell gezeigte Halle und ein etwaiger Filter. Ändert sie
 * sich, stimmt das Raster nicht mehr und es wird neu gebaut.
 *
 * Der Rotationsstand gehört dazu: Beim Umschalten der Halle sind die Felder
 * dieselben, nur eine andere Gruppe steht auf dem Schirm — ohne ihn hielte
 * der Patch das für „nichts geändert" und die Rotation bliebe stehen.
 *
 * @param {Array<object>} courts
 * @param {number} hallIdx Laufender Rotationszähler der Anzeige.
 * @param {string} hallFilter Fest eingestellte Halle (`?halle=`), sonst leer.
 * @returns {string}
 */
export function sichtSignatur(courts, hallIdx, hallFilter) {
  const list = Array.isArray(courts) ? courts : [];
  const felder = list.map((c) => `${c.court_id}:${c.location || ""}`).join("|");
  return `${felder}#${hallIdx || 0}#${hallFilter || ""}`;
}

/**
 * Hat sich am Satzstand dieses Felds überhaupt etwas geändert? Nur dann muss
 * seine Karte angefasst werden — sonst tauschte ein Punkt auf Feld 3 die
 * Ziffern **aller** Felder aus und gäbe einen großen Teil der Ersparnis
 * wieder her.
 *
 * @param {object} vorher
 * @param {object} nachher
 * @returns {boolean}
 */
export function satzstandGleich(vorher, nachher) {
  const a = Array.isArray(vorher && vorher.sets) ? vorher.sets : [];
  const b = Array.isArray(nachher && nachher.sets) ? nachher.sets : [];
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (satzWert(a[i], 0) !== satzWert(b[i], 0)) return false;
    if (satzWert(a[i], 1) !== satzWert(b[i], 1)) return false;
  }
  return true;
}
