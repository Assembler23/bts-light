// Hängt die turnier.de-GUID als `&g=` an einen öffentlichen badhub-Live-Link
// (ADR 0054) — Schwester von `src-tauri/src/aushang.rs::link_mit_guid`.
//
// Ohne den Anhang zeigt ein Link, der aus der Verbands-`live_url` gebaut ist,
// auf den Verbandsstand statt auf das konkrete Kind-Turnier — bei zwei
// parallelen BVBB-Turnieren also aufs falsche.

/** Hängt `g=<guid>` an `url` an — vor bereits vorhandenen Parametern wie
 *  `display=` oder `halle=`, damit die Reihenfolge egal ist (badhub liest
 *  Query-Parameter, keine Positionen). Getrimmte, leere Adresse bleibt leer
 *  (der Aufrufer meldet damit „keine Live-Seite"); ohne GUID bleibt `url`
 *  unverändert. */
export function linkMitGuid(url, guid) {
  const trimmed = String(url ?? "").trim();
  if (!trimmed || !guid) return trimmed;
  const trenner = trimmed.includes("?") ? "&" : "?";
  return `${trimmed}${trenner}g=${guid}`;
}
