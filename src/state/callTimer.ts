// Aufruf-Timer-Logik (1./2./3. Aufruf), geteilt von Spielübersicht und
// Ansagen-Seite. Der 1. Aufruf ist das Aufrufen aufs Feld; ab den in der
// Config hinterlegten Minuten gilt der 2. bzw. 3./letzte Aufruf als fällig.
import { useEffect, useState } from "react";
import type { CallTimerConfig } from "../types";

export type CallTone = "ok" | "warn" | "due";

export interface CallInfo {
  /** Sekunden seit dem 1. Aufruf. */
  elapsedSec: number;
  /** Hochzählende Uhr als „m:ss". */
  clock: string;
  /** Aktuell höchster fälliger Aufruf (1/2/3). */
  stage: 1 | 2 | 3;
  /** Beschriftung des Aufruf-Chips. */
  label: string;
  /** Ampelton: grün (ok) / gelb (warn) / rot (due). */
  tone: CallTone;
}

/** Sekunden als „m:ss". */
export function formatClock(sec: number): string {
  const s = Math.max(0, Math.floor(sec));
  const m = Math.floor(s / 60);
  return `${m}:${String(s % 60).padStart(2, "0")}`;
}

/** Aufruf-Stand für ein belegtes Feld berechnen. */
export function callInfo(
  onCourtSinceMs: number,
  nowMs: number,
  cfg: CallTimerConfig,
): CallInfo {
  const elapsedSec = Math.max(0, (nowMs - onCourtSinceMs) / 1000);
  const min = elapsedSec / 60;
  let stage: 1 | 2 | 3 = 1;
  let label = "1. Aufruf";
  let tone: CallTone = "ok";
  if (min >= cfg.third_call_minutes) {
    stage = 3;
    label = "Letzter Aufruf";
    tone = "due";
  } else if (min >= cfg.second_call_minutes) {
    stage = 2;
    label = "2. Aufruf fällig";
    tone = "warn";
  }
  return { elapsedSec, clock: formatClock(elapsedSec), stage, label, tone };
}

/**
 * Tickt im angegebenen Takt und liefert die aktuelle Zeit (ms) – damit die
 * hochzählende Uhr jede Sekunde aktualisiert, unabhängig vom Daten-Poll.
 */
export function useNow(intervalMs = 1000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(id);
  }, [intervalMs]);
  return now;
}
