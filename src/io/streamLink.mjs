/** Adresse der Stream-Einblendung für den Baukasten im Court-Monitor-Panel
 *  (Spec `docs/features/monitor-stile-und-stream.md`, ADR 0069).
 *
 *  Gegenstück zu `courtsAusQuery`/`optionenAusQuery` in `streamAnsicht.mjs`:
 *  Was hier gebaut wird, muss die Seite genau so wieder lesen. Optionen
 *  erscheinen nur als `=0` (abschalten) — eingeschaltet ist der Normalfall
 *  und braucht keinen Parameter.
 */

import { MAX_FELDER } from "./streamAnsicht.mjs";

/** `base` = Wurzel ohne Schrägstrich am Ende (`http://bts-light.local:8088`
 *  oder `https://badhub.de/bts-relay/<Kennung>`). Ohne Feld → "". */
export function streamLink(base, courtIds, opt) {
  var ids = [];
  (courtIds || []).forEach(function (id) {
    if (Number.isInteger(id) && id > 0 && ids.indexOf(id) < 0 && ids.length < MAX_FELDER) {
      ids.push(id);
    }
  });
  if (!base || ids.length === 0) return "";
  var o = opt || {};
  var url = base.replace(/\/+$/, "") + "/stream?courts=" + ids.join(",");
  if (o.nation === false) url += "&nation=0";
  if (o.verein === false) url += "&verein=0";
  if (o.logo === false) url += "&logo=0";
  return url;
}
