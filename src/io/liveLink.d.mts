// Typdeklaration für den Turnier-GUID-Anhang an Live-Links (liveLink.mjs).

/** Hängt `g=<guid>` an `url` an (vor bereits vorhandenen Parametern). Leere
 *  Adresse bleibt leer; ohne GUID bleibt `url` unverändert. */
export declare function linkMitGuid(
  url: string | undefined,
  guid: string | undefined,
): string;
