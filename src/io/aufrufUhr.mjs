/** Aufruf-Uhr „Zeit seit Aufruf" in der Kopfzeile des Zähl-Tablets.
 *
 *  Der Court-Monitor zeigt seit v0.9.54 eine hochzählende Uhr ab dem
 *  1. Feld-Aufruf samt Ampel „1. Aufruf → 2. Aufruf → Letzter Aufruf". Das
 *  Tablet zeigt dieselbe Uhr, damit der Bediener am Feld ohne Blick zum TV
 *  weiß, wie lange die Spieler schon gerufen sind — gerade während der
 *  Seitenwahl, wenn noch niemand zählt.
 *
 *  Regeln (bewusst identisch zu `monitor.html` `renderCallTimer`):
 *
 *  - Gated durch die App-Einstellung **Aufruf-Timer** (`callTimer.enabled`).
 *    Ist der Timer aus, bleibt die Uhr leer — Tablet und Monitore zeigen
 *    dasselbe.
 *  - Grundlage ist `on_court_since_ms` (Zeitpunkt des 1. Feld-Aufrufs) aus
 *    der `/health`-Antwort, gerechnet gegen die **Server-Zeit**, nicht die
 *    Tablet-Uhr.
 *  - Eine Schwelle ≤ 0 oder ohne Wert schaltet die jeweilige Stufe ab
 *    (älterer Server, der sie nicht schickt).
 *  - **Anders als am Monitor:** Sobald das Tablet im Spiel ist
 *    (`gestartet`: Aufstellung bestätigt, Spieldauer läuft — oder Punkte
 *    stehen, auch serverseitig bekannte), weicht die Uhr der Spieldauer in
 *    der Kopfzeile. Zwei Uhren nebeneinander wären mehr Verwirrung als
 *    Nutzen.
 *
 *  Kanonische Fassung. `tablet.html` trägt eine Inline-Kopie (die Assets
 *  durchlaufen keinen Build und können keine Module laden) — Änderungen hier
 *  und dort gemeinsam.
 */

/**
 * Text und Ampelstufe der Aufruf-Uhr — oder `null`, wenn sie nicht
 * erscheinen soll.
 *
 * @param {number} jetztMs Server-Zeit (Unix-ms).
 * @param {number|null|undefined} onCourtSinceMs Zeitpunkt des 1. Aufrufs (Unix-ms); ≤ 0 = keiner.
 * @param {{enabled?:boolean, secondCallMinutes?:number, thirdCallMinutes?:number}|null|undefined} callTimer
 *   Aufruf-Timer-Umschlag aus `/health` (camelCase wie im MonitorState).
 * @param {boolean} gestartet Ist das Tablet schon im Spiel?
 * @returns {null | {uhr: string, label: string, stufe: "ok"|"warn"|"due"}}
 */
export function aufrufUhr(jetztMs, onCourtSinceMs, callTimer, gestartet) {
  if (!callTimer || !callTimer.enabled) return null;
  if (typeof onCourtSinceMs !== "number" || !(onCourtSinceMs > 0)) return null;
  if (gestartet) return null;
  const sec = Math.max(0, Math.floor((jetztMs - onCourtSinceMs) / 1000));
  const uhr = Math.floor(sec / 60) + ":" + ("0" + (sec % 60)).slice(-2);
  const min = sec / 60;
  const zweite = callTimer.secondCallMinutes;
  const dritte = callTimer.thirdCallMinutes;
  let label = "1. Aufruf";
  let stufe = "ok";
  if (typeof dritte === "number" && dritte > 0 && min >= dritte) {
    label = "Letzter Aufruf";
    stufe = "due";
  } else if (typeof zweite === "number" && zweite > 0 && min >= zweite) {
    label = "2. Aufruf";
    stufe = "warn";
  }
  return { uhr, label, stufe };
}

/**
 * Steht in der Satzliste eines Felds schon ein Punkt? LAN liefert
 * `[[a,b]]`, das Relay `[{a,b}]` — beide Formen gelten.
 *
 * @param {any} sets
 * @returns {boolean}
 */
function punkteGespielt(sets) {
  if (!Array.isArray(sets)) return false;
  return sets.some((s) => {
    if (Array.isArray(s)) return (s[0] > 0) || (s[1] > 0);
    return !!s && ((s.a > 0) || (s.b > 0));
  });
}

/**
 * Das eigene Feld aus der `/health`-Antwort herausgreifen.
 *
 * LAN (`/health`) und Cloud (`/{ns}/health`) liefern gleich: `courts[]` mit
 * `court_id`, `match_id`, `sets` + `on_court_since_ms`, daneben `callTimer`.
 * Der schmale Abruf (`?court=<id>`) bringt nur das eine Feld — die Suche
 * nach der ID schützt trotzdem vor einem älteren Server (etwa der
 * Slave-Brücke), der den Filter nicht kennt und alle Felder schickt.
 *
 * `gefunden: false` heißt „der Server kennt das Feld (noch) nicht" — etwa
 * ein Relay direkt nach dem Neustart, bevor der Host seine Feldliste
 * hochgeladen hat. Der Aufrufer behält dann seinen letzten Stand, statt die
 * Uhr auf eine leere Antwort hin auszublenden.
 *
 * @param {any} antwort Geparste JSON-Antwort (oder `null` bei Fehler).
 * @param {number} courtId Stabile BTP-CourtID dieses Tablets.
 * @returns {{gefunden: boolean, matchId: number, onCourtSinceMs: number|null, gespielt: boolean, callTimer: object|null}}
 */
export function feldAusHealth(antwort, courtId) {
  const leer = { gefunden: false, matchId: 0, onCourtSinceMs: null, gespielt: false, callTimer: null };
  if (!antwort || typeof antwort !== "object") return leer;
  const callTimer = (antwort.callTimer && typeof antwort.callTimer === "object")
    ? antwort.callTimer : null;
  const courts = Array.isArray(antwort.courts) ? antwort.courts : [];
  const feld = courts.find((c) => c && c.court_id === courtId);
  if (!feld) return { ...leer, callTimer };
  const stempel = typeof feld.on_court_since_ms === "number" ? feld.on_court_since_ms : null;
  const matchId = typeof feld.match_id === "number" ? feld.match_id : 0;
  return { gefunden: true, matchId, onCourtSinceMs: stempel, gespielt: punkteGespielt(feld.sets), callTimer };
}
