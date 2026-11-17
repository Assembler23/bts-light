// Ziele der badhub-Links in der Turnierleitungs-Sicht (`assets/tl.html`).
//
// Der Turnier-PC meldet im TL-Zustand `badhub = { base, guid?, liveticker? }`
// (`tablet/tl.rs` `badhub_links`): die Basis folgt der Push-Adresse — ein
// Turnier auf test.badhub.de verlinkt also auf test.badhub.de. Mit der
// Turnier-GUID führen die Links über die badhub-Weiterleitung
// `/checkin/<GUID>/turnier` auf Turnierseite, Konkurrenz und Spieleransicht
// genau DIESES Turniers (die Turnierseiten-Adresse selbst kennt bts-light
// nicht).
//
// tl.html trägt eine Inline-Kopie (kein Build-Schritt);
// `scripts/test-badhub-links.mjs` vergleicht ihr Verhalten mit diesem Modul.

/** Basis der Links. Hinter dem Cloud-Relay (`praefix` nicht leer) immer der
 *  Host, von dem die Seite kommt — der Relay liegt auf demselben badhub wie
 *  der Liveticker (badhub.de bzw. test.badhub.de), und ein Turnier-PC soll
 *  die Links einer badhub-Seite nicht auf einen fremden Host lenken können
 *  (Security-Review 17.11.2026). Im Hallennetz der Wert des Hosts, nur als
 *  `http(s)://host[:port]`; fehlt er (älterer Host), das Produktivsystem. */
export function badhubBasis(links, praefix, origin) {
  if (praefix !== "") return origin;
  const b = links && typeof links.base === "string" ? links.base : "";
  return /^https?:\/\/[A-Za-z0-9.\-]+(:\d+)?$/.test(b) ? b : "https://badhub.de";
}

/** Weiterleitung `…/checkin/<GUID>/turnier[?…]`, "" ohne gültige GUID.
 *  `angaben` nimmt nur `spiel`, `konk` und `player`; leere fallen weg. */
export function turnierSprung(links, basis, angaben) {
  const guid = links && typeof links.guid === "string" ? links.guid : "";
  if (!/^[0-9A-Fa-f]{8}(-[0-9A-Fa-f]{4}){3}-[0-9A-Fa-f]{12}$/.test(guid)) return "";
  const q = new URLSearchParams();
  for (const k of ["spiel", "konk", "player"]) {
    const v = angaben && angaben[k] != null ? String(angaben[k]).trim() : "";
    if (v) q.set(k, v);
  }
  const s = q.toString();
  return `${basis}/checkin/${guid}/turnier${s ? "?" + s : ""}`;
}

/** Spielerseite zu einer Lizenznummer: im Turnier (Spieleransicht), ohne
 *  GUID die globale Weiterleitung `/spieler/<Nr>/live`. "" ohne Nummer. */
export function spielerLink(links, basis, nummer) {
  const nr = String(nummer ?? "").trim();
  if (!nr) return "";
  return turnierSprung(links, basis, { player: nr })
    || `${basis}/spieler/${encodeURIComponent(nr)}/live`;
}

/** Konkurrenz eines Spiels auf der Turnierseite (über die BTP-Spiel-ID, der
 *  Name ist badhubs Rückfall). Nur mit GUID — sonst gibt es kein Ziel. */
export function konkurrenzLink(links, basis, matchId, konk) {
  const id = Number(matchId);
  if (!Number.isSafeInteger(id) || id <= 0) return "";
  return turnierSprung(links, basis, { spiel: id, konk });
}

/** Öffentlicher Liveticker aus dem Host-Zustand — nur auf derselben Basis
 *  wie alle übrigen Links, sonst keiner. */
export function livetickerLink(links, basis) {
  const u = links && typeof links.liveticker === "string" ? links.liveticker.trim() : "";
  return /^https?:\/\/[^\s"'<>]+$/.test(u) && u.startsWith(basis + "/") ? u : "";
}
