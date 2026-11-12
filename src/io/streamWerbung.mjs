/** Logik der Stream-Werbung (Spec `docs/features/stream-werbung.md`, ADR 0092):
 *  zweite OBS-Ebene mit den für den Stream angehakten Werbebildern.
 *
 *  Aus der Adresse kommen nur Darstellung, Hintergrund und Band-Tempo — jeder
 *  unbekannte Wert fällt auf die Vorgabe zurück. Welche Bilder erscheinen,
 *  entscheidet allein das Häkchen „Stream" im Setup (`streamAds`).
 *
 *  Kanonische Fassung. `stream-werbung.html` trägt eine Inline-Kopie (die
 *  Assets durchlaufen keinen Build) — Änderungen hier und dort gemeinsam;
 *  `scripts/test-stream-werbung.mjs` vergleicht beide wortgleich.
 */

export var MODI = ["rotation", "reihe", "band"];
export var HINTERGRUENDE = ["transparent", "kachel"];
export var TEMPO_PX_S = { langsam: 40, mittel: 80, schnell: 140 };
export var VORGABE = { modus: "rotation", hintergrund: "transparent", tempo: "mittel" };
export var ABSTAND_PX = 24;
export var INNENRAND_PX = 8;
export var ABRUF_MS = 20000;
export var KACHEL_ERSATZ = "#000000";

/** Optionen aus der Adresse; Unbekanntes → Vorgabe. */
export function optionenAusQuery(modus, hintergrund, tempo) {
  return {
    modus: MODI.indexOf(modus) >= 0 ? modus : VORGABE.modus,
    hintergrund: HINTERGRUENDE.indexOf(hintergrund) >= 0 ? hintergrund : VORGABE.hintergrund,
    tempo: Object.prototype.hasOwnProperty.call(TEMPO_PX_S, tempo) ? tempo : VORGABE.tempo,
  };
}

/** Was tun mit einer Antwort? Nur „200 mit Liste `streamAds`" wird
 *  angewandt (auch eine leere Liste — dann leert sich die Ebene). Alles
 *  andere — 503 „keine Daten", Fehler, alter Relay ohne Feld — heißt:
 *  stehen lassen, was steht (ADR 0092). */
export function entscheidung(status, json) {
  if (status === 200 && json && Array.isArray(json.streamAds)) return "anwenden";
  return "behalten";
}

/** Kachelfarbe eines Bildes: die Setup-Farbe, sonst Schwarz. Index über
 *  `ads` — passt für LAN-Dateinamen und Cloud-Indizes gleichermaßen. Nur
 *  `#rrggbb` geht in ein `style`. */
export function stilFarbe(ads, adStyles, id) {
  var i = Array.isArray(ads) ? ads.indexOf(id) : -1;
  var stil = i >= 0 && Array.isArray(adStyles) ? adStyles[i] : null;
  var bg = stil && typeof stil.bg === "string" ? stil.bg : "";
  return /^#[0-9a-f]{6}$/.test(bg) ? bg : KACHEL_ERSATZ;
}

/** Bild-Adresse; `ver` (nur Cloud) bricht den Bild-Cache, wenn Indizes
 *  nachrücken. */
export function bildUrl(base, id, ver) {
  return base + "ads/" + encodeURIComponent(id) + (ver ? "?v=" + encodeURIComponent(ver) : "");
}

/** Die Stream-Bilder aus dem Zustand, in Setup-Reihenfolge. */
export function streamBilder(state, base) {
  var ids = state && Array.isArray(state.streamAds) ? state.streamAds : [];
  var ver = state && Array.isArray(state.streamVer) ? state.streamVer : [];
  var bilder = [];
  for (var i = 0; i < ids.length; i++) {
    var id = String(ids[i]);
    var v = typeof ver[i] === "string" ? ver[i] : "";
    bilder.push({
      id: id,
      url: bildUrl(base, id, v),
      bg: stilFarbe(state.ads, state.adStyles, ids[i]),
    });
  }
  return bilder;
}

/** Schlüssel einer Bildliste: Ändert er sich, wird neu aufgebaut — sonst
 *  liefe das Band nach jedem Abruf von vorn. */
export function listenSchluessel(bilder) {
  return bilder.map(function (b) { return b.url + "|" + b.bg; }).join("\n");
}

/** „nebeneinander": gemeinsam verkleinern, bis die Reihe passt; nie größer. */
export function reiheMassstab(gesamtBreite, verfuegbar) {
  if (!(gesamtBreite > 0) || !(verfuegbar > 0)) return 1;
  return Math.min(1, verfuegbar / gesamtBreite);
}

/** Band: wie oft die Bildfolge hintereinander stehen muss, damit eine Hälfte
 *  der Bahn die Quelle füllt — sonst klafft bei wenigen Bildern eine Lücke. */
export function bandWiederholungen(kopieBreite, sichtBreite) {
  if (!(kopieBreite > 0)) return 1;
  return Math.max(1, Math.ceil(sichtBreite / kopieBreite));
}

/** Dauer eines Umlaufs (eine Hälfte der Bahn) in Sekunden. */
export function bandDauerS(kopieBreite, wiederholungen, pxS) {
  if (!(pxS > 0)) return 0;
  return (kopieBreite * wiederholungen) / pxS;
}

/** Wechsel-Intervall der Rotation; mindestens 1 s. */
export function rotationIntervallMs(intervalS) {
  var s = typeof intervalS === "number" && intervalS > 0 ? Math.max(1, intervalS) : 10;
  return Math.round(s * 1000);
}
