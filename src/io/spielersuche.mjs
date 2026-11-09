// Spielersuche in der Turnierleitungs-Oberfläche (Spec `tl-spielersuche`,
// ADR 0081): reine Entscheidungs- und Textlogik der Trefferliste.
//
// Kanonische Fassung; `src-tauri/assets/tl.html` trägt eine Inline-Kopie
// (die Seite wird ohne Build-Schritt ausgeliefert).
// `scripts/test-spielersuche.mjs` prüft beide auf gleiches Verhalten.

/**
 * Lohnt eine Anfrage? Mindestens zwei Zeichen, die keine Trennzeichen sind —
 * dieselben Trenner, die der Turnier-PC beim Falten als Leerraum zählt. Die
 * genaue Prüfung (nach der Faltung, „ue" = ein Zeichen) macht der Host.
 */
export function suchbereit(q) {
  const zeichen = Array.from(String(q ?? "")).filter(
    (c) => !/\s/.test(c) && !"-,'./’".includes(c),
  );
  return zeichen.length >= 2 && Array.from(String(q)).length <= 64;
}

/**
 * Wohin führt ein Tipp auf einen Treffer? Die Reihenfolge der Prüfungen ist
 * gewollt: Was gar nicht in der Liste stehen KANN (Panel ausgeblendet,
 * hinter dem Deckel), wird nicht erst durch Filterumstellen versprochen.
 *
 * `lage`:
 * - `panelSichtbar`, `panelZu`: Zustand des Panels im aktiven Profil.
 * - `inListe`: Steht das Spiel im aktuellen TL-Zustand dieser Liste?
 * - `platz`: Platz laut Suchantwort (für den Hinweistext).
 * - `halle`, `hallFilter`: Halle des Treffers und gewählter Filter.
 * - `mpVerborgen`: Filter „nur spielbereite" verbirgt die Zeile.
 *
 * Ergebnis: `{art: "unsichtbar", platz}` · `{art: "halle", halle}` ·
 * `{art: "filterMp"}` · `{art: "panelZu"}` · `{art: "zeile"}`.
 */
export function sprungZiel(gruppe, lage) {
  if (!lage.panelSichtbar || !lage.inListe) {
    return { art: "unsichtbar", platz: lage.platz || null };
  }
  const filter = lage.hallFilter || "";
  const halle = lage.halle || "";
  // Die Beendet-Liste kennt keinen Hallenfilter; die Warteliste zeigt
  // hallenlose Spiele in jeder Halle (wie `visibleQueue`).
  const hallePasst =
    gruppe === "finished" ||
    !filter ||
    halle === filter ||
    (gruppe === "waiting" && !halle);
  if (!hallePasst) return { art: "halle", halle };
  if (gruppe === "waiting" && lage.mpVerborgen) return { art: "filterMp" };
  if (lage.panelZu) return { art: "panelZu" };
  return { art: "zeile" };
}

/** Rollen der gesuchten Person als kurze Marken. */
export function rollenText(roles) {
  const namen = { player: "Spieler", sr: "SR", ar: "AR", candidate: "möglicher Teilnehmer" };
  return (Array.isArray(roles) ? roles : []).map((r) => (Object.hasOwn(namen, r) ? namen[r] : "")).filter(Boolean);
}

/** Klartext einer gescheiterten Suche nach HTTP-Status (0 = Netz/Frist). */
export function fehlerText(status) {
  if (status === 400) return "Bitte anders suchen – 2 bis 64 Zeichen, höchstens 6 Wörter.";
  if (status === 401) return "Dieses Gerät ist nicht mehr freigegeben.";
  if (status === 429) return "Zu viele Anfragen – kurz warten.";
  return "Turnier-PC nicht erreichbar.";
}

/** Text „weitere N – bitte genauer suchen", leer ohne Rest. */
export function mehrText(more) {
  return more > 0 ? `weitere ${more} – bitte genauer suchen` : "";
}
