/** Meetingpoint-Seite (Spec `docs/features/meetingpoint-spielbereit.md`):
 *  reine Logik der Seite `src-tauri/assets/meetingpoint.html`.
 *
 *  Kanonische Fassung. Die Seite trägt Inline-Kopien (die Assets durchlaufen
 *  keinen Build); `scripts/test-meetingpoint.mjs` schneidet sie heraus und
 *  vergleicht ihr Verhalten mit diesem Modul. Bewusst ES5-nah geschrieben
 *  (`var`, `function`), damit die Kopie wortgleich bleiben kann — die Seite
 *  läuft auch auf alten Hallen-TVs.
 */

/** Halle aus der Query (`?halle=…`); leer, wenn keine angegeben ist. */
export function mpHalleAusQuery(search) {
  var s = String(search || "");
  if (s.charAt(0) === "?") s = s.slice(1);
  var teile = s.split("&");
  for (var i = 0; i < teile.length; i++) {
    var kv = teile[i].split("=");
    if (kv[0] === "halle" && kv.length > 1) {
      try {
        return decodeURIComponent(kv.slice(1).join("=").replace(/\+/g, " ")).trim();
      } catch (e) {
        return "";
      }
    }
  }
  return "";
}

/** Partien in Aufrufreihenfolge (ältester Aufruf oben), stabil nach ID. */
export function mpSortiert(partien) {
  var liste = (partien || []).slice();
  liste.sort(function (a, b) {
    var d = (a.calledAtMs || 0) - (b.calledAtMs || 0);
    return d !== 0 ? d : (a.matchId || 0) - (b.matchId || 0);
  });
  return liste;
}

/** Plätze einer Partie nach Seite 1, Seite 2 und Schiedsrichter. */
export function mpGruppen(partie) {
  var g = { team1: [], team2: [], sr: null };
  var plaetze = (partie && partie.plaetze) || [];
  for (var i = 0; i < plaetze.length; i++) {
    var p = plaetze[i];
    var art = p && p.platz && p.platz.art;
    if (art === "spieler" && p.platz.seite === 1) g.team1.push(p);
    else if (art === "spieler" && p.platz.seite === 2) g.team2.push(p);
    else if (art === "sr") g.sr = p;
  }
  return g;
}

/** Wie viele der für „spielbereit" nötigen Plätze sind eingecheckt? Der
 *  SR zählt nur mit, wenn er nötig ist (`srNoetig`) — sonst wiche die Seite
 *  vom Zähler der Turnierleitung ab. */
export function mpZaehler(partie) {
  var plaetze = (partie && partie.plaetze) || [];
  var srNoetig = !!(partie && partie.srNoetig);
  var da = 0, gesamt = 0;
  for (var i = 0; i < plaetze.length; i++) {
    var art = plaetze[i].platz && plaetze[i].platz.art;
    if (art === "sr" && !srNoetig) continue;
    gesamt++;
    if (plaetze[i].present) da++;
  }
  return { da: da, gesamt: gesamt };
}

/** Body eines Check-ins (gleiche Form in LAN, Cloud und am Slave). */
export function mpCheckinBody(halle, matchId, platz, present) {
  return {
    halle: String(halle || ""),
    matchId: matchId,
    platz: platz,
    present: !!present,
    ort: "meetingpoint",
  };
}

/** Uhrzeit des Aufrufs als „HH:MM" (Ortszeit des Geräts). */
export function mpUhrzeit(ms) {
  if (!ms) return "";
  var d = new Date(ms);
  var h = d.getHours(), m = d.getMinutes();
  return (h < 10 ? "0" : "") + h + ":" + (m < 10 ? "0" : "") + m;
}
