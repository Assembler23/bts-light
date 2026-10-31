// App-weiter Status des Ansage-Tonwegs (Spec `ansage-ausgabegeraet`):
// Der Announcer meldet hier, wenn der Kern nicht spielen konnte (alter Weg
// sprang ein) oder das gewählte Gerät fehlte (Ersatz über den Standard).
// Anders als der Azure-Hinweis lösen sich diese Hinweise selbst auf.
// Mehrere zugleich möglich — sonst verdrängten sich „Gerät fehlt" (Gong)
// und „keine Stimme" (Sprache) bei jeder Ansage gegenseitig.
// Bewusst ohne Context/Props: Announcer (io/) ist kein React-Code.
import { useSyncExternalStore } from "react";
import type { TonBanner } from "../io/tonWeg.mjs";

let aktuell: readonly TonBanner[] = [];
const hoerer = new Set<() => void>();

function setzen(next: TonBanner[]): void {
  if (
    next.length === aktuell.length &&
    next.every((a, i) => a === aktuell[i])
  ) {
    return;
  }
  aktuell = next;
  hoerer.forEach((h) => h());
}

/** Vom Announcer gerufen. Loggt nur die Art — nie einen Gerätenamen
 *  (Datenschutz, Diagnose-Log). */
export function meldeAusgabeProblem(art: TonBanner): void {
  if (aktuell.includes(art)) return;
  console.warn("Ansage-Ausgabe:", art);
  setzen([...aktuell, art]);
}

/** Ein Stück lief wieder über den Kern: alle Rückfall-Hinweise weg —
 *  „Gerät fehlt" bleibt, das löst nur das gewählte Gerät selbst. */
export function loeseAusgabeProblem(): void {
  setzen(aktuell.filter((a) => a === "geraet_fehlt"));
}

/** Das gewählte Gerät ist wieder da bzw. die Wahl wurde zurückgestellt. */
export function loeseGeraetFehlt(): void {
  setzen(aktuell.filter((a) => a !== "geraet_fehlt"));
}

export function useAusgabeProbleme(): readonly TonBanner[] {
  return useSyncExternalStore(
    (h) => {
      hoerer.add(h);
      return () => {
        hoerer.delete(h);
      };
    },
    () => aktuell,
  );
}
