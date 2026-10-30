// Entscheidungen der Ansage-Seite im Browser (Spec `ansage-im-browser`) —
// rein, ohne DOM und ohne Ton, damit sie testbar sind.
//
// `assets/ansage.html` trägt dieses Modul als Inline-Kopie zwischen den
// Markern `<ansageSeite.mjs>` (ohne `export`); `scripts/test-ansage-seite.mjs`
// hält beide wortgleich.

/** Abfrage-Takt der Seite. */
export const ANSAGE_TAKT_MS = 1500;
/** Was länger als eine Minute in der Warteschlange liegt, verfällt — eine
 *  Seite, die offline war, spricht beim Wiederkommen nichts nach. */
export const ANSAGE_VERFALL_MS = 60000;

/** Die Arten, die diese Seite abspielt. Eine unbekannte (aus einer neueren
 *  Fassung des Turnier-PCs) wird übersprungen. */
export const ANSAGE_ARTEN = [
  "court_call", "prep_call", "officials", "start_play", "scorekeeper_call",
  "freetext", "desktop", "court_assigned",
];

/** Hallenvergleich wie im Kern (`halle_passt`): Rand und Schreibweise zählen
 *  nicht, ein leeres Ziel gilt allen. */
export function hallePasst(ziel, geraet) {
  const z = String(ziel || "").trim().toLowerCase();
  const g = String(geraet || "").trim().toLowerCase();
  return z === "" || g === "" || z === g;
}

/** Ab zwei Hallen ist die Wahl Pflicht: „keine Halle" hieße „alle" — und
 *  damit fremde Aufrufe in dieser Halle. */
export function halleNoetig(halls) {
  return Array.isArray(halls) && halls.length >= 2;
}

/** Darf die Seite mit dieser Halle loslegen? Bei Pflicht nur mit einer
 *  Halle, die es im Turnier gibt. */
export function halleGueltig(halls, gewaehlt) {
  if (!halleNoetig(halls)) return true;
  const g = String(gewaehlt || "").trim();
  return g !== "" && halls.some((h) => hallePasst(h, g) && String(h).trim() !== "");
}

/**
 * Nimmt einen Feed entgegen und sagt, was neu zu spielen ist.
 *
 * `stand` = `{ lastId, bestand }`. Der **erste** Feed ist Bestandsaufnahme:
 * Was schon da war, wird nicht gesprochen — wer die Seite öffnet, will nicht
 * die letzte Minute nachhören.
 */
export function neueEintraege(stand, feed) {
  const eintraege = Array.isArray(feed && feed.eintraege) ? feed.eintraege : [];
  let lastId = stand.lastId || 0;
  const neu = [];
  for (const e of eintraege) {
    if (!e || typeof e.id !== "number" || e.id <= lastId) continue;
    if (stand.bestand && ANSAGE_ARTEN.indexOf(e.art) >= 0) neu.push(e);
  }
  for (const e of eintraege) {
    if (e && typeof e.id === "number" && e.id > lastId) lastId = e.id;
  }
  neu.sort((a, b) => a.id - b.id);
  return { stand: { lastId, bestand: true }, neu };
}

/** Passt ein Eintrag noch zur Belegung? Mit Spiel-Bezug nur, wenn genau
 *  dieses Spiel auf genau diesem Feld steht. */
export function belegungPasst(e, belegung) {
  if (e.courtId == null || e.matchId == null) return true;
  return (belegung || []).some(
    (b) => b.courtId === e.courtId && b.matchId === e.matchId,
  );
}

/** Kurz vor dem Abspielen: noch frisch genug und noch richtig? Die Frist
 *  zählt ab Empfang auf diesem Gerät — die Uhr des Geräts muss dafür nicht
 *  zur Uhr des Turnier-PCs passen. */
export function nochAbspielbar(e, belegung, empfangenMs, jetztMs) {
  return jetztMs - empfangenMs < ANSAGE_VERFALL_MS && belegungPasst(e, belegung);
}

/** Womit diese Seite spricht: Audio vom Turnier-PC, die eigene
 *  Gerätestimme — oder gar nicht. */
export function stimme(hostAudio, geraeteStimme) {
  if (hostAudio) return "host";
  if (geraeteStimme) return "geraet";
  return "keine";
}

/** Meldet sich die Seite als Ansage-Gerät ihrer Halle? Nur, wenn wirklich
 *  etwas erklingen kann — sonst glaubte die Turnierleitung, ihr Aufruf
 *  komme an. */
export function meldetPraesenz(feed, geraeteStimme, tonFrei) {
  return !!(feed && feed.aktiv) && !!tonFrei &&
    stimme(feed.hostAudio, geraeteStimme) !== "keine";
}

/** Text zum `grund` eines nicht aktiven Feeds. */
export function grundText(grund) {
  switch (grund) {
    case "schalter_aus":
      return "Der Turnier-PC erzeugt keine Ansagen für diese Seite (Schalter „Audio für Ansage-Seiten erzeugen“ ist aus).";
    case "slave":
      return "Dieser Rechner ist ein Ansage-Slave — die Seite gehört an den Turnier-PC.";
    case "renderer_weg":
      return "Die Oberfläche von bts-light am Turnier-PC antwortet nicht — ist das Fenster geöffnet?";
    case "turnier_pc_offline":
      return "Der Turnier-PC ist nicht erreichbar.";
    default:
      return "Der Turnier-PC erzeugt gerade keine Ansagen.";
  }
}

/** Adresse des Feeds. `base` endet mit `/`. */
export function feedAdresse(base, halle, since, hoert) {
  return base + "ansage/feed?halle=" + encodeURIComponent(halle || "") +
    "&since=" + (since || 0) + (hoert ? "&hoert=1" : "");
}

/* ── Verlauf + Großanzeige (Erweiterung 30.10.2026) ───────────────────── */

/** So viele gespielte Ansagen zeigt die Seite `/ansage` — und hält für
 *  ebenso viele das geladene Audio zum Wiederholen im Speicher. */
export const ANSAGE_VERLAUF_MAX = 5;

/** Lesbarer Text einer Ansage: ihre Segmente, zusammengefügt. */
export function ansageText(e) {
  const segmente = Array.isArray(e && e.segmente) ? e.segmente : [];
  return segmente
    .map((s) => String(s == null ? "" : s).trim())
    .filter((s) => s !== "")
    .join(" ")
    .replace(/\s+/g, " ");
}

/**
 * Nimmt eine gerade gespielte Ansage in den Verlauf auf: neueste vorn,
 * höchstens `max` Einträge. Gibt eine neue Liste zurück; der Eintrag
 * `{ id, ms, text, e, puffer }` selbst wird nicht kopiert, damit das Audio
 * später an ihm landen kann. Dieselbe Ansage (gleiche `id`) steht nie zweimal
 * im Verlauf — eine Wiederholung ist kein neuer Eintrag.
 */
export function verlaufAufnehmen(verlauf, eintrag, max) {
  const deckel = typeof max === "number" ? max : ANSAGE_VERLAUF_MAX;
  const alt = (Array.isArray(verlauf) ? verlauf : []).filter((v) => v && v.id !== eintrag.id);
  return [eintrag].concat(alt).slice(0, Math.max(0, deckel));
}

/** Auftrag für die Warteschlange, der eine Ansage aus dem Verlauf noch
 *  einmal spielt — `null`, wenn es sie (nicht mehr) gibt. Eine Wiederholung
 *  hat der Nutzer bewusst angefordert: Sie verfällt nicht und wird nicht
 *  gegen die Belegung geprüft. */
export function wiederholungsAuftrag(verlauf, id) {
  const v = (Array.isArray(verlauf) ? verlauf : []).filter((x) => x && x.id === id)[0];
  if (!v) return null;
  return { e: v.e, wiederholung: true, verlauf: v };
}

/**
 * Größte Schrift (ganze Pixel) zwischen `min` und `max`, bei der `passt(px)`
 * noch wahr ist — Halbierungssuche, `passt` misst im Browser. Passt nicht
 * einmal `min`, bleibt es bei `min`.
 */
export function schriftSuchen(min, max, passt) {
  let unten = Math.floor(min);
  let oben = Math.floor(max);
  if (oben <= unten || !passt(unten)) return unten;
  if (passt(oben)) return oben;
  while (oben - unten > 1) {
    const mitte = Math.floor((unten + oben) / 2);
    if (passt(mitte)) unten = mitte;
    else oben = mitte;
  }
  return unten;
}
