// App-weiter Status der „aufgerufenen Spiele" der eigenen Halle (Cluster C
// Stufe 2). Der CloudAnnounceSlave-Poll holt sie ohnehin bei jedem Tick aus
// dem Cloud-Relay; er veröffentlicht sie hier, damit die Ansagen-Seite sie
// anzeigen kann, OHNE denselben Command ein zweites Mal zu pollen (ein Poll,
// zwei Verbraucher). Am Master bleibt die Liste leer.
import { useSyncExternalStore } from "react";
import type { CloudPrepared } from "../types";

let value: CloudPrepared[] = [];
const listeners = new Set<() => void>();

/** Vom CloudAnnounceSlave-Poll gesetzt. Referenzgleiche leere Liste wird
 *  wiederverwendet, damit „nichts aufgerufen" keinen Dauer-Re-Render auslöst. */
export function setPreparedGames(next: CloudPrepared[]): void {
  if (next.length === 0 && value.length === 0) return;
  value = next;
  listeners.forEach((l) => l());
}

export function usePreparedGames(): CloudPrepared[] {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => {
        listeners.delete(l);
      };
    },
    () => value,
  );
}
