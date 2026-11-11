// Geparkte Ergebnisse des Zähl-Tablets (Spec tablet-stand-robust, Regel C;
// ADR 0085).
//
// Ein Ergebnis, das der Turnier-PC noch nicht angenommen hat, darf weder
// verloren gehen noch das nächste Spiel blockieren. Bisher tat es das
// zweite: Der offene Auftrag überlebte den Spielwechsel, und „Ergebnis
// übermitteln" am Folgespiel lehnte ab, solange er anlag.
//
// Wechselt das Spiel auf dem Feld (oder wird das Feld geleert), wandert der
// offene Auftrag deshalb in eine eigene, kleine Liste:
//
// - eigener Speicher-Schlüssel je Feld — sie überlebt das Leeren des
//   Spielstands und reist NICHT im gespiegelten Zustand;
// - sie wird im Hintergrund weiter zugestellt (nach 15 s, dann alle 60 s,
//   und bei jeder Neuverbindung), bis der Turnier-PC „angenommen" oder
//   „überholt" sagt;
// - solange etwas darin liegt, zeigt das Tablet ein Band — das Ergebnis ist
//   sichtbar, statt still zu verschwinden;
// - kommt das geparkte Spiel auf dieses Feld zurück, wird NICHT still
//   gesendet, sondern gefragt (das Spiel wird womöglich neu gespielt).
//
// Ein Eintrag trägt nur, was der Auftrag ohnehin trägt (Match-ID, Feld,
// Sätze, Aufgabe/kampflos) plus die Spielnummer fürs Band — keine Namen.
//
// `src-tauri/assets/tablet.html` trägt eine Inline-Kopie (kein Modul-Import
// möglich); `scripts/test-inline-kopien.mjs` vergleicht ihr Verhalten mit
// diesem Modul. Deshalb: schlichte `function`-Deklarationen.

/** Mehr Einträge hält die Liste nicht; die ältesten fallen heraus. Fünf
 *  unzugestellte Ergebnisse an einem Feld sind schon ein Störfall. */
export const GEPARKT_MAX = 5;
/** Mehr Sätze trägt kein Ergebnis. Der Eintrag kommt aus dem
 *  `localStorage` und geht als Body an den Turnier-PC — ein aufgeblähter
 *  Eintrag soll gar nicht erst geladen werden. */
export const GEPARKT_MAX_SAETZE = 7;
/** Erste Nachprüfung nach dem Parken. */
export const GEPARKT_ERSTE_PRUEFUNG_MS = 15000;
/** Danach in diesem Takt. */
export const GEPARKT_TAKT_MS = 60000;

/** So lange bleibt ein frisch geparkter Eintrag UNSICHTBAR. Im Normalfall
 *  überholt das `match_cleared` (WebSocket) nur die Antwort auf das Ergebnis
 *  (HTTP) — die kommt Augenblicke später und löst den Eintrag auf. Ohne
 *  Karenz blitzte nach jedem Spiel kurz „nicht zugestellt" auf. */
export const GEPARKT_KARENZ_MS = 4000;

/** Hat `e` die Form eines geparkten Eintrags? Die Liste kommt aus dem
 *  `localStorage` — beschädigte Einträge dürfen nichts werfen. */
export function geparktGueltig(e) {
  return !!e && typeof e === "object" && !Array.isArray(e)
    && !!e.auftrag && typeof e.auftrag === "object" && !Array.isArray(e.auftrag)
    && typeof e.auftrag.matchId === "number" && isFinite(e.auftrag.matchId)
    && Array.isArray(e.auftrag.sets) && e.auftrag.sets.length <= GEPARKT_MAX_SAETZE;
}

/** Liest die gespeicherte Liste (JSON-Text). Unlesbares ergibt eine leere
 *  Liste; je Spiel bleibt nur der jüngste Eintrag. */
export function geparktLaden(roh) {
  var liste;
  try { liste = JSON.parse(roh); } catch (e) { return []; }
  if (!Array.isArray(liste)) return [];
  var aus = [];
  for (var i = 0; i < liste.length; i++) {
    if (!geparktGueltig(liste[i])) continue;
    aus = aufloesen(aus, liste[i].auftrag.matchId);
    aus.push(liste[i]);
  }
  return aus.slice(-GEPARKT_MAX);
}

/** Muss der offene Auftrag geparkt werden, wenn `matchId` (oder nichts:
 *  `null`) aufs Feld kommt? Ja, sobald er zu einem ANDEREN Spiel gehört. */
export function mussParken(auftrag, matchId) {
  return !!auftrag && typeof auftrag === "object" && auftrag.matchId !== matchId;
}

/** Nimmt `auftrag` in die Liste auf (neue Liste; ein älterer Eintrag zum
 *  selben Spiel wird ersetzt). */
export function parken(liste, auftrag, matchNr, jetztMs) {
  var aus = aufloesen(liste, auftrag.matchId);
  aus.push({
    auftrag: auftrag,
    matchNr: typeof matchNr === "number" || typeof matchNr === "string" ? matchNr : null,
    seitMs: jetztMs,
  });
  return aus.slice(-GEPARKT_MAX);
}

/** Die Liste ohne den Eintrag zu `matchId` (neue Liste). */
export function aufloesen(liste, matchId) {
  var aus = [];
  for (var i = 0; i < liste.length; i++) {
    if (liste[i].auftrag.matchId !== matchId) aus.push(liste[i]);
  }
  return aus;
}

/** Der geparkte Eintrag zu `matchId`, sonst `null`. */
export function geparktFuer(liste, matchId) {
  if (matchId == null) return null;
  for (var i = 0; i < liste.length; i++) {
    if (liste[i].auftrag.matchId === matchId) return liste[i];
  }
  return null;
}

/** Die Einträge, die im Hintergrund zugestellt werden dürfen: alle AUSSER
 *  dem Spiel, das gerade auf dem Feld liegt — dafür wird gefragt. Hat der
 *  Bediener auf die Frage „Jetzt senden" geantwortet (`freigegeben`), geht
 *  auch dieser Eintrag raus. */
export function zuPruefen(liste, aktuellesMatchId) {
  var aus = [];
  for (var i = 0; i < liste.length; i++) {
    var e = liste[i];
    if (e.freigegeben === true || aktuellesMatchId == null || e.auftrag.matchId !== aktuellesMatchId) aus.push(e);
  }
  return aus;
}

/** „Jetzt senden": Der Eintrag zu `matchId` darf zugestellt werden, obwohl
 *  sein Spiel auf dem Feld liegt (neue Liste; der Auftrag selbst bleibt
 *  dasselbe Objekt). Der Eintrag bleibt GEPARKT, bis der Turnier-PC ihn
 *  annimmt — er wandert bewusst nicht in den Sendeauftrag des laufenden
 *  Spiels, sonst sperrte er dessen eigenes Ergebnis. */
export function freigeben(liste, matchId) {
  var aus = [];
  for (var i = 0; i < liste.length; i++) {
    var e = liste[i];
    aus.push(e.auftrag.matchId === matchId
      ? { auftrag: e.auftrag, matchNr: e.matchNr, seitMs: e.seitMs, freigegeben: true }
      : e);
  }
  return aus;
}

/** Die Einträge, die das Band zeigen darf: alles, was die Karenz hinter
 *  sich hat (oder keinen Zeitstempel trägt). */
export function geparktSichtbar(liste, jetztMs) {
  var aus = [];
  for (var i = 0; i < liste.length; i++) {
    var seit = liste[i].seitMs;
    if (typeof seit !== "number" || seit <= jetztMs - GEPARKT_KARENZ_MS) aus.push(liste[i]);
  }
  return aus;
}

/** Wann die nächste Hintergrund-Prüfung fällig ist. */
export function naechstePruefungMs(schonGeprueft) {
  return schonGeprueft ? GEPARKT_TAKT_MS : GEPARKT_ERSTE_PRUEFUNG_MS;
}

/** Das Ergebnis eines Auftrags in Worten: „21:10, 21:15", „kampflos",
 *  „21:10, 5:2, Aufgabe". */
export function saetzeText(auftrag) {
  var teile = [];
  var sets = auftrag && Array.isArray(auftrag.sets) ? auftrag.sets : [];
  for (var i = 0; i < sets.length; i++) {
    var s = sets[i];
    if (s && typeof s.a === "number" && typeof s.b === "number") teile.push(s.a + ":" + s.b);
  }
  if (auftrag && auftrag.walkover) return "kampflos";
  if (auftrag && auftrag.retired) teile.push("Aufgabe");
  return teile.length ? teile.join(", ") : "ohne Sätze";
}

/** „Spiel Nr. 12" — oder, ohne Nummer, „einem früheren Spiel". */
export function spielName(e) {
  return e.matchNr != null && e.matchNr !== "" ? "Spiel Nr. " + e.matchNr : "einem früheren Spiel";
}

/** Text des Hinweisbands; leer, wenn nichts geparkt ist. Gezeigt wird der
 *  älteste Eintrag, weitere nur als Zahl. */
export function bandText(liste) {
  if (!liste.length) return "";
  var e = liste[0];
  var mehr = liste.length > 1 ? " (+" + (liste.length - 1) + " weitere)" : "";
  return "Ergebnis von " + spielName(e) + " (" + saetzeText(e.auftrag)
    + ") nicht zugestellt — Turnierleitung informieren" + mehr;
}

/** Frage, wenn das geparkte Spiel auf dieses Feld zurückkommt. */
export function frageText(e) {
  return "Für dieses Spiel liegt ein nicht zugestelltes Ergebnis vor (" + saetzeText(e.auftrag) + ").";
}

/** Zweite Abfrage vor dem Verwerfen — sie zeigt das Ergebnis. */
export function verwerfenText(e) {
  return "Ergebnis von " + spielName(e) + " (" + saetzeText(e.auftrag)
    + ") wirklich verwerfen? Es steht dann nicht in BTP.";
}
