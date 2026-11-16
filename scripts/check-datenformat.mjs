// Vergleicht release/datenformat.json eines PRs mit dem Stand des
// Zielzweigs (Spec hotfix-ohne-neustart, ADR 0101).
//
// Der Rust-Test `abdruck_weicht_ab_macht_rot` erzwingt nur, dass der Abdruck
// stimmt. Er kann nicht sehen, was VORHER in der Datei stand. Diese Prüfung
// schließt die Lücke:
//
//   1. Die Grenze `rueckwaerts_lesbar_ab` sinkt nie. Ein kleinerer Wert
//      behauptete, eine Version von VOR einem früheren Bruch könne die
//      heutigen Daten lesen.
//   2. Ändert sich der Abdruck, ändert sich auch die Begründung — sonst hat
//      jemand nur die Zahl aus der Fehlermeldung kopiert, ohne zu
//      entscheiden.
//
// Aufruf: node scripts/check-datenformat.mjs <alt.json> <neu.json>
// Fehlt die alte Datei (erster PR mit der Datei), ist nichts zu vergleichen.
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { istVersion, vergleicheVersion } from "./build-kern-kopf.mjs";

/** Liste der Verstöße; leer = in Ordnung. */
export function pruefeWechsel(alt, neu) {
  const fehler = [];
  if (!istVersion(neu?.rueckwaerts_lesbar_ab)) {
    return ["rueckwaerts_lesbar_ab ist keine Version X.Y.Z."];
  }
  if (!alt) return fehler;
  if (
    istVersion(alt.rueckwaerts_lesbar_ab) &&
    vergleicheVersion(neu.rueckwaerts_lesbar_ab, alt.rueckwaerts_lesbar_ab) < 0
  ) {
    fehler.push(
      `rueckwaerts_lesbar_ab sinkt von ${alt.rueckwaerts_lesbar_ab} auf ${neu.rueckwaerts_lesbar_ab}. ` +
        "Die Grenze darf nur gleich bleiben oder steigen.",
    );
  }
  const norm = (s) => String(s ?? "").trim();
  if (norm(alt.abdruck) !== norm(neu.abdruck) && norm(alt.begruendung) === norm(neu.begruendung)) {
    fehler.push(
      "Der Abdruck hat sich geändert, die Begründung nicht. Bitte in `begruendung` " +
        "festhalten, warum die Grenze bleibt oder steigt.",
    );
  }
  return fehler;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [altPfad, neuPfad] = process.argv.slice(2);
  const lesen = (p) => (p && existsSync(p) ? JSON.parse(readFileSync(p, "utf8")) : null);
  let alt = null;
  try {
    alt = lesen(altPfad);
  } catch {
    // Ein unlesbarer alter Stand ist kein Grund, den neuen abzulehnen.
  }
  const fehler = pruefeWechsel(alt, lesen(neuPfad));
  for (const f of fehler) console.error(`Fehler: ${f}`);
  if (fehler.length === 0) console.error("release/datenformat.json: in Ordnung.");
  process.exit(fehler.length === 0 ? 0 : 1);
}
