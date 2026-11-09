/** Meetingpoint-Eintrag in der Kiosk-Feldwahl `/felder`
 *  (Spec docs/features/meetingpoint-am-kiosk-tablet.md).
 *
 *  Reine Entscheidungslogik, damit sie testbar ist. Kanonische Fassung:
 *  `lobby.html` trägt Inline-Kopien aller Funktionen, `anzeige.html` eine von
 *  `ohneMeetingpointMarke` — `scripts/test-inline-kopien.mjs` prüft, dass sie
 *  sich wie hier verhalten. Bewusst ES5-nah geschrieben (`var`), damit die
 *  Kopien wörtlich übernommen werden können.
 *
 *  Die Marke „zuletzt Meetingpoint" ist der vorhandene Merker der
 *  Anzeige-Hülle (`badhub.anzeige.zuletzt`, `{layout, court, halle}`) — kein
 *  zweiter Schlüssel, der mit ihm auseinanderlaufen könnte.
 */

/** Der localStorage-Schlüssel der Hülle (anzeige.html `ZULETZT_KEY`). */
export var ZULETZT_KEY = "badhub.anzeige.zuletzt";

/** Wohin der Eintrag führt. Ohne `halle`: Der Hallen-Merker der Hülle greift. */
export function meetingpointZiel() {
  return "/anzeige?layout=meetingpoint";
}

/** Ist der Meetingpoint an? 200 → ja, 404 → aus (Route liefert dann 404);
 *  alles andere (Netzfehler = 0, Frist, 5xx) lässt den bisherigen Stand —
 *  ein Aussetzer soll den Eintrag nicht flackern lassen. */
export function mpAusStatus(status, bisher) {
  if (status === 200) return true;
  if (status === 404) return false;
  return !!bisher;
}

function merkerLesen(roh) {
  if (typeof roh !== "string" || roh === "") return null;
  var o;
  try { o = JSON.parse(roh); } catch (e) { return null; }
  return o && typeof o === "object" && !Array.isArray(o) ? o : null;
}

/** Stand das Gerät zuletzt als Meetingpoint? (Hinweis-Knopf in der Lobby) */
export function warZuletztMeetingpoint(roh) {
  var o = merkerLesen(roh);
  return !!o && o.layout === "meetingpoint";
}

/** Merker ohne Meetingpoint-Marke: nur `layout` wird zur Zähltafel, Feld und
 *  Halle bleiben (die Halle braucht der nächste Meetingpoint-Start). `null`,
 *  wenn nichts zu ändern ist. */
export function ohneMeetingpointMarke(roh) {
  var o = merkerLesen(roh);
  if (!o || o.layout !== "meetingpoint") return null;
  return JSON.stringify({ layout: "tafel", court: o.court === undefined ? null : o.court, halle: o.halle === undefined ? null : o.halle });
}
