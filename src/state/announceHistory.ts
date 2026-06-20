// Verlauf der zuletzt manuell ausgelösten Ansagen (Freitext + manuelle
// Feld-Ansage) zum Nachschlagen und erneuten Abspielen. Bewusst nur die
// selbst ausgelösten Ansagen – automatische Spielaufrufe landen NICHT hier.
// Persistiert in localStorage (übersteht einen Reload), gedeckelt auf die
// letzten zehn Einträge.
import { useSyncExternalStore } from "react";
import type { CourtOverview } from "../types";

export interface AnnounceHistoryEntry {
  /** Fortlaufende ID (neueste zuerst). */
  id: number;
  /** Freitext-Ansage oder manuelle Feld-Ansage. */
  kind: "freetext" | "field";
  /** Angezeigter Text (Freitext bzw. „Feld – Paarung"). */
  text: string;
  /** Ziel-Halle (leer = alle Hallen); bei Feld-Ansage die Halle des Felds. */
  hall: string;
  /** Zeitpunkt (ms) der Ansage. */
  ts: number;
  /** Feld-Schnappschuss zum erneuten Abspielen einer Feld-Ansage. */
  court?: CourtOverview;
}

const KEY = "bts_announce_history";
const MAX = 10;

function load(): AnnounceHistoryEntry[] {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as AnnounceHistoryEntry[]) : [];
  } catch {
    return [];
  }
}

let entries: AnnounceHistoryEntry[] = load();
const listeners = new Set<() => void>();

function persist() {
  try {
    localStorage.setItem(KEY, JSON.stringify(entries));
  } catch {
    /* localStorage nicht verfügbar – Verlauf bleibt nur im Speicher */
  }
}

/** Eine ausgelöste Ansage protokollieren (neueste zuerst, auf MAX gedeckelt). */
export function recordAnnounce(
  entry: Omit<AnnounceHistoryEntry, "id" | "ts">,
): void {
  const id = (entries[0]?.id ?? 0) + 1;
  entries = [{ ...entry, id, ts: Date.now() }, ...entries].slice(0, MAX);
  persist();
  for (const l of listeners) l();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function snapshot(): AnnounceHistoryEntry[] {
  return entries;
}

/** React-Hook: liefert den aktuellen Verlauf und re-rendert bei Änderungen. */
export function useAnnounceHistory(): AnnounceHistoryEntry[] {
  return useSyncExternalStore(subscribe, snapshot);
}
