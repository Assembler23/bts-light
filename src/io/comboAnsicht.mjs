/** Die zwei Entscheidungen der Kombi-Anzeige (Spec
 *  `docs/features/kombi-ausrichtung-je-monitor.md`, ADR 0049).
 *
 *  **Wann hat sich die Zuweisung geändert?** Die Seite vergleicht im
 *  Sekundentakt ihre eigene Adresse mit dem `redirectTo` des Hosts und
 *  navigiert bei Abweichung neu. Drei Query-Parameter dürfen dabei nicht
 *  zählen: `device` (hängt die Seite selbst an), `rotate` (steht in der
 *  Kiosk-URL, nie im Ziel — ohne diese Ausnahme navigiert ein hochkant
 *  montierter TV endlos) und `dir` (stand bis v0.9.269 im Ziel; ohne die
 *  Ausnahme lüde beim Update jeder migrierte TV einmal komplett neu).
 *
 *  **Steht die Anzeige nebeneinander?** Der Host schickt die Ausrichtung im
 *  laufenden `/combo/state`-Poll. Sagt er nichts — weil die Seite ohne
 *  `?device=` läuft und ihm damit unbekannt ist —, behält der Startwert aus
 *  der URL (`?dir=v`) das letzte Wort.
 *
 *  Kanonische Fassung. `combo.html` trägt eine Inline-Kopie (die Assets
 *  durchlaufen keinen Build und können keine Module laden) — Änderungen hier
 *  und dort gemeinsam.
 */

/** Query-Parameter, die beim Adressvergleich nicht zählen. */
export const IGNORIERTE_PARAMS = ["device", "rotate", "dir"];

/** Zeigt die Seite schon das, worauf `redirectTo` verweist? */
export function urlPasst(hierHref, zielPfad, origin) {
  try {
    const hier = new URL(hierHref);
    const dort = new URL(zielPfad, origin);
    for (const p of IGNORIERTE_PARAMS) {
      hier.searchParams.delete(p);
      dort.searchParams.delete(p);
    }
    return (
      hier.pathname === dort.pathname &&
      hier.searchParams.toString() === dort.searchParams.toString()
    );
  } catch (e) {
    // Unlesbares Ziel: lieber als Unterschied behandeln und navigieren, als
    // auf einer womöglich veralteten Seite festzuhängen.
    return false;
  }
}

/** Ausrichtung für diesen Render-Durchlauf: Server schlägt Startwert — aber
 *  nur, wenn er sich äußert. Alles außer einem echten Boolean gilt als
 *  „keine Ansage". */
export function ausrichtungVertikal(state, startVertikal) {
  if (state && typeof state.vertical === "boolean") return state.vertical;
  return !!startVertikal;
}
