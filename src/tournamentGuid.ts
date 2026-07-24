/** Turnier-GUID von turnier.de — Schlüssel des Hallen-Check-Ins (ADR 0009).
 *
 *  Spiegelt `config::is_tournament_uuid` im Rust-Kern. Die Prüfung ist bewusst
 *  nachsichtig (Groß-/Kleinschreibung, Leerzeichen, BTPs `{…}`-Schreibweise),
 *  denn sie soll den Tippfehler abfangen, nicht über Formalien belehren. */

const GUID_RE = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i;

/** Ist der Wert eine wohlgeformte Turnier-GUID (8-4-4-4-12 Hex)? */
export function isTournamentGuid(value: string): boolean {
  const trimmed = value.trim().replace(/^\{|\}$/g, "");
  return new RegExp(`^${GUID_RE.source}$`, "i").test(trimmed);
}

/** Holt die GUID aus einer Eingabe heraus.
 *
 *  Turnierleiter kopieren in aller Regel die ganze Adresse aus dem Browser
 *  (`https://www.turnier.de/tournament/<GUID>/matches`), nicht die GUID
 *  daraus. Statt das als Fehler abzuweisen, wird sie herausgezogen — auch aus
 *  `{…}`-Schreibweise oder mit Leerzeichen drumherum.
 *
 *  Gibt `""` zurück, wenn nichts Verwertbares drinsteht. */
export function extractTournamentGuid(value: string): string {
  const match = value.match(GUID_RE);
  return match ? match[0].toUpperCase() : "";
}
