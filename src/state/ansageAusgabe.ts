// App-weiter Status des Ansage-Tonwegs (Spec `ansage-ausgabegeraet`):
// Der Announcer meldet hier, wenn der Kern nicht spielen konnte und der
// alte Weg im Programmfenster einspringen musste. Anders als der
// Azure-Hinweis löst sich dieses Banner selbst auf, sobald das nächste
// Stück wieder über den Kern lief — der Zustand ist dann vorbei.
// Bewusst ohne Context/Props: Announcer (io/) ist kein React-Code.
import { useSyncExternalStore } from "react";
import type { TonBanner } from "../io/tonWeg.mjs";

let aktuell: TonBanner | null = null;
const hoerer = new Set<() => void>();

function setzen(next: TonBanner | null): void {
  if (next === aktuell) return;
  aktuell = next;
  hoerer.forEach((h) => h());
}

/** Vom Announcer gerufen, wenn ein Stück über den Rückfall lief. Loggt nur
 *  die Art — nie einen Gerätenamen (Datenschutz, Diagnose-Log). */
export function meldeAusgabeProblem(art: TonBanner): void {
  if (art !== aktuell) {
    console.warn("Ansage-Ausgabe: Rückfall auf das Programmfenster:", art);
  }
  setzen(art);
}

/** Vom Announcer gerufen, wenn ein Stück wieder über den Kern lief. */
export function loeseAusgabeProblem(): void {
  setzen(null);
}

export function useAusgabeProblem(): TonBanner | null {
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
