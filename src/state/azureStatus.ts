// Winziger App-weiter Status rund um die Azure-Ansage (ADR 0003):
//  1) die vom Master geerbte Azure-Stimme (Cloud-Slave) — gesetzt vom
//     CloudAnnounceSlave-Poll, angezeigt in den Ansage-Einstellungen;
//  2) der letzte Azure→Web-Speech-Fallback — gesetzt vom Announcer,
//     angezeigt als Banner, damit der Rückfall nicht mehr stumm passiert.
// Bewusst ohne Context/Props: Announcer (io/) ist kein React-Code.
import { useSyncExternalStore } from "react";

function makeStore<T>(initial: T) {
  let value = initial;
  const listeners = new Set<() => void>();
  return {
    get: () => value,
    set(next: T) {
      if (Object.is(next, value)) return;
      value = next;
      listeners.forEach((l) => l());
    },
    subscribe(l: () => void) {
      listeners.add(l);
      return () => {
        listeners.delete(l);
      };
    },
  };
}

// --- 1) Vom Master geerbte Azure-Stimme (nur Cloud-Slave, sonst null) ---

const inheritedVoice = makeStore<string | null>(null);

export function setInheritedAzureVoice(voice: string | null): void {
  inheritedVoice.set(voice);
}

export function useInheritedAzureVoice(): string | null {
  return useSyncExternalStore(inheritedVoice.subscribe, inheritedVoice.get);
}

// --- 2) Letzter Azure-Fallback (Azure fehlgeschlagen → Standardstimme) ---

export interface AzureFallbackInfo {
  /** Fehlertext des fehlgeschlagenen Azure-Aufrufs. */
  message: string;
  /** Zeitpunkt (Unix-ms) — unterscheidet wiederholte gleiche Fehler. */
  at: number;
}

const fallback = makeStore<AzureFallbackInfo | null>(null);

/** Vom Announcer gerufen, wenn Azure fehlschlägt und die Web-Speech-
 *  Standardstimme übernimmt. Loggt zusätzlich in die Konsole (Diagnose-Log). */
export function reportAzureFallback(err: unknown): void {
  const message = err instanceof Error ? err.message : String(err);
  console.warn("Azure-Ansage fehlgeschlagen, Standardstimme übernimmt:", message);
  fallback.set({ message, at: Date.now() });
}

/** Banner-Quittierung: Hinweis ausblenden, bis ein neuer Fehler auftritt. */
export function dismissAzureFallback(): void {
  fallback.set(null);
}

export function useAzureFallback(): AzureFallbackInfo | null {
  return useSyncExternalStore(fallback.subscribe, fallback.get);
}
