// Reihenfolge der GERUFENEN Spiele in der TL-Spielliste (Spec
// `meetingpoint-reihenfolge`, ADR 0077): Unter Gerufenen gilt die
// Aufruf-Reihenfolge, umsortierbar nur innerhalb derselben Aufruf-Halle.
// Diese Funktionen übersetzen einen Zug oder Knopfdruck in das Ziel von
// `called_reorder` (`beforeMatchId`; `null` = ans Ende der Gerufenen).
// Inline-Kopie in src-tauri/assets/tl.html (Drift-Test
// scripts/test-gerufene-reihe.mjs).

/** Die gerufenen Spiele derselben Aufruf-Halle wie `matchId`, in Listenfolge. */
export function rufReihe(liste, matchId) {
  var eigen = null;
  for (var i = 0; i < (liste || []).length; i++) {
    if (liste[i].match_id === matchId) { eigen = liste[i]; break; }
  }
  if (!eigen || !eigen.prep_call) return [];
  var halle = eigen.prep_call.hall || "";
  return liste
    .filter(function (m) { return m.prep_call && (m.prep_call.hall || "") === halle; })
    .map(function (m) { return m.match_id; });
}

/** Ziel eines Zugs: abgelegt vor `beforeId` (`null` = ans Listenende).
 *  Das nächste gerufene Spiel derselben Halle ab der Ablagestelle; gibt es
 *  keins, ans Ende der Gerufenen (`null`). `undefined` = keine Bewegung. */
export function rufZielZug(liste, matchId, beforeId) {
  var reihe = rufReihe(liste, matchId);
  if (!reihe.length) return undefined;
  var ab = beforeId == null ? liste.length
    : liste.findIndex(function (m) { return m.match_id === Number(beforeId); });
  if (ab < 0) ab = liste.length;
  var ziel = null;
  for (var i = ab; i < liste.length; i++) {
    var id = liste[i].match_id;
    if (id !== matchId && reihe.indexOf(id) >= 0) { ziel = id; break; }
  }
  // Steht das Spiel schon genau vor diesem Ziel (bzw. schon am Ende), ist
  // es keine Bewegung.
  var pos = reihe.indexOf(matchId);
  var naechstes = pos + 1 < reihe.length ? reihe[pos + 1] : null;
  return ziel === naechstes ? undefined : ziel;
}

/** Ziel eines Knopfdrucks (`top`, `up`, `down`, `end`); `undefined` = nichts zu tun. */
export function rufZielKnopf(liste, matchId, art) {
  var reihe = rufReihe(liste, matchId);
  var i = reihe.indexOf(matchId);
  if (i < 0) return undefined;
  var letzte = i >= reihe.length - 1;
  if (art === "top") return i === 0 ? undefined : reihe[0];
  if (art === "up") return i === 0 ? undefined : reihe[i - 1];
  if (art === "down") return letzte ? undefined : (i + 2 < reihe.length ? reihe[i + 2] : null);
  if (art === "end") return letzte ? undefined : null;
  return undefined;
}
