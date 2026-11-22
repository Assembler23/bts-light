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

/** `99F55FAB-E301-…` → „99F55FAB…“ — so kürzen auch der Rust-Kern
 *  (`badhub::tp_guid::kurz`) und die badhub-Meldungen beide Kennungen. */
export function shortTournamentGuid(guid: string): string {
  return `${guid.slice(0, 8)}…`;
}

/** Wirksame Turnier-GUID (kanonisch, groß) — Spiegel von Rust
 *  `AppConfig::tournament_uuid_kanonisch`: Bei Quelle badhub mit gekoppeltem
 *  Turnier die GUID der Kopplung (`guid`, sonst `uuid`), sonst das Feld
 *  „Turnier bei turnier.de“. `""` ohne gültige GUID. */
export function wirksameTurnierGuid(c: {
  quelle?: string;
  tournament_uuid: string;
  badhub_turnier?: { uuid: string; basis: string; guid?: string } | null;
  punktspiel?: { uuid: string; basis: string } | null;
}): string {
  // Punktspiel: die Kennung der Begegnung ist die GUID des Livetickers.
  const p = c.punktspiel;
  if (c.quelle === "punktspiel" && p && p.basis.trim() !== "" && isTournamentGuid(p.uuid)) {
    return extractTournamentGuid(p.uuid);
  }
  const t = c.badhub_turnier;
  if (c.quelle === "badhub" && t && t.uuid.trim() !== "" && t.basis.trim() !== "") {
    const g = [t.guid ?? "", t.uuid].find((v) => isTournamentGuid(v));
    if (g) return extractTournamentGuid(g);
  }
  return isTournamentGuid(c.tournament_uuid) ? extractTournamentGuid(c.tournament_uuid) : "";
}
