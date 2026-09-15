/** Frist für den Stand-Abruf der Anzeige-Seiten.
 *  (Turnier 05./06.09.2026, LAN, sechs Pi-Monitore hinter Router + Access-Point)
 *
 *  Die Monitore froren phasenweise ein — ohne Offline-Blende, während Tablets
 *  und TL-Web am selben Turnier-PC sauber weiterliefen. Der Unterschied liegt
 *  im Client: Ein Tablet hält eine WebSocket-Verbindung mit Ping/Pong und
 *  reißt sie nach 15 s Stille selbst ab. Eine Anzeige holt ihren Stand per
 *  `fetch`, und dieser Abruf hatte **keinen Timeout**.
 *
 *  Geht eine Antwort im WLAN-Roaming verloren, bleibt der Abruf offen. Bei
 *  `monitor`, `overview`, `tafel` und `tl` hält dann der In-Flight-Schutz
 *  jeden weiteren Poll zurück, bei `combo`, `winners`, `preparation` und
 *  `lobby` hängt der nächste Abruf am Ende des vorigen — in beiden Fällen
 *  kommt nie wieder ein Stand. Der WebSocket erkennt den toten Kanal zwar
 *  nach 25 s und verbindet neu, aber sein einziger Effekt ist ein erneuter
 *  Aufruf desselben blockierten Abrufs. Erst die TCP-Sendewiederholung des
 *  Geräts löst den Knoten, und deren Abstand wächst bis auf rund zwei
 *  Minuten. Daher „Phasen" von Minuten, obwohl das Netz längst wieder steht.
 *
 *  **Die Frist macht aus dem Hänger einen Fehler.** Ein Fehler ist der Weg,
 *  den die Seiten längst beherrschen: Offline-Blende, Kanal ungesund,
 *  schneller Takt, nächster Abruf auf einer frischen Verbindung.
 *
 *  Vier Entscheidungen:
 *
 *  1. **Zwei Budgets.** Bis zu den Kopfzeilen 5 s — im Hallennetz sind es
 *     Millisekunden, am Relay hält der Zwischenspeicher die Übersicht eine
 *     Viertelsekunde vor. Für den Rumpf danach noch einmal 15 s: Die
 *     Feld-Übersicht ist unkomprimiert rund 16 KB, und eine ferne Halle am
 *     LTE-Hotspot oder ein Pi am WLAN-Rand darf langsam sein, ohne dass eine
 *     **lebende** Antwort abgeschnitten wird. Zusammen bleiben beide unter
 *     den 25 s, nach denen der WebSocket einen toten Kanal aufgibt
 *     (`HERZSCHLAG_STILL_MS`).
 *  2. **Der Helfer nimmt den Leser entgegen**, damit die Frist bis zum Ende
 *     des Rumpfs gilt — und damit im Fall 3 nichts Verspätetes mehr in die
 *     Seite läuft.
 *  3. **Sie wirkt auch ohne `AbortController`** (Wettlauf gegen einen
 *     Timer). Der Grund sind die Fernseher-Browser älterer Smart-TVs, die
 *     `/alle` und `/next` direkt laden (siehe `docs/court-monitor.md`,
 *     TV-Launcher); ihr Chromium-Stand kennt keinen `AbortController`. Dort
 *     hängt der alte Abruf zwar weiter im Browser, aber die Seite ist frei
 *     für den nächsten — das ist der eigentliche Zweck. Eine später doch
 *     noch eintreffende Antwort wird **verworfen**, der Leser läuft nicht
 *     mehr: Sonst räumte ein toter Abruf im Nachhinein die Blende ab oder
 *     löste den In-Flight-Schutz für einen fremden, laufenden Abruf.
 *  4. **Sie bricht wirklich ab**, wo sie kann. Ein abgebrochener Abruf gibt
 *     seine Verbindung frei; der nächste beginnt mit einem neuen
 *     Verbindungsaufbau, der nach einem Roaming sofort durchgeht.
 *
 *  **Die Leser der Seiten bleiben frei von Nebenwirkungen.** Sie reichen
 *  Stand und ETag-Marke nur durch; Entwarnung, Marke und In-Flight-Schutz
 *  werden erst in der äußeren Kette gesetzt — die läuft nur, wenn die Frist
 *  gehalten hat. Sonst räumte im Rückfall ohne `AbortController` ein Rumpf,
 *  der nach der Frist doch noch eintrifft, die Blende ab und merkte sich die
 *  Marke eines nie gezeigten Standes (Review-Fund 08.09.2026).
 *
 *  Kanonische Fassung. `monitor.html`, `overview.html`, `tafel.html`,
 *  `combo.html`, `winners.html`, `preparation.html`, `lobby.html`, `ad.html`
 *  und `tl.html` tragen eine Inline-Kopie ohne den `fetchFn`-Parameter (die
 *  Assets durchlaufen keinen Build); der Test hält die neun Kopien
 *  byte-gleich und ihre Budgets auf denen des Moduls.
 */

/** Bis wann die Kopfzeilen da sein müssen. */
export const KOPF_FRIST_MS = 5000;

/** Wie lange der Rumpf nach den Kopfzeilen noch brauchen darf. */
export const RUMPF_FRIST_MS = 15000;

/**
 * Führt einen Abruf samt Lesen des Rumpfs aus und beendet ihn spätestens nach
 * Ablauf der Fristen als Fehler.
 *
 * @param {Function} fetchFn `fetch` oder ein Ersatz mit gleicher Signatur.
 * @param {string} url Adresse.
 * @param {object|undefined} opt Optionen wie für `fetch`; werden kopiert, nicht verändert.
 * @param {Function|null} lesen Verarbeitet die Antwort (z. B. `r => r.json()`);
 *   ohne Leser kommt die rohe Antwort zurück.
 * @param {number} [kopfMs] Frist bis zu den Kopfzeilen; Standard {@link KOPF_FRIST_MS}.
 * @param {number} [rumpfMs] Frist für den Rumpf danach; Standard {@link RUMPF_FRIST_MS}.
 * @returns {Promise<*>} Ergebnis des Lesers — oder ein Fehler.
 */
export function abrufMitFrist(fetchFn, url, opt, lesen, kopfMs, rumpfMs) {
  const kopf = kopfMs > 0 ? kopfMs : KOPF_FRIST_MS;
  const rumpf = rumpfMs > 0 ? rumpfMs : RUMPF_FRIST_MS;
  const ctrl = typeof AbortController === "function" ? new AbortController() : null;
  const optionen = Object.assign({}, opt);
  if (ctrl) optionen.signal = ctrl.signal;
  let timer = null;
  let abgelaufen = false;
  let abbrechen = null;
  const abbruch = new Promise((_, reject) => {
    abbrechen = (was) => {
      abgelaufen = true;
      if (ctrl) ctrl.abort();
      reject(new Error("Abruf-Frist überschritten (" + was + ")"));
    };
    timer = setTimeout(() => abbrechen("Kopfzeilen " + kopf + " ms"), kopf);
  });
  const abruf = fetchFn(url, optionen).then((antwort) => {
    // Nur ohne AbortController erreichbar: Der Wettlauf ist längst
    // entschieden, die Seite hat den Abruf abgeschrieben — der Leser darf
    // seine Nebenwirkungen nicht mehr entfalten.
    if (abgelaufen) throw new Error("Antwort nach Ablauf der Frist verworfen");
    clearTimeout(timer);
    timer = setTimeout(() => abbrechen("Rumpf " + rumpf + " ms"), rumpf);
    return lesen ? lesen(antwort) : antwort;
  });
  // `race` hängt an beiden Zweigen einen Abnehmer — ein spät verwerfender
  // Abruf nach dem Abbruch löst damit keine `unhandledrejection` aus.
  return Promise.race([abruf, abbruch]).then(
    (wert) => { clearTimeout(timer); return wert; },
    (fehler) => { clearTimeout(timer); throw fehler; },
  );
}
