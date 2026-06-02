// Hochzählende Aufruf-Uhr + Aufruf-Chip (1. ✓ → 2. fällig → letzter) für ein
// belegtes Feld. Grün/gelb/rot je nach Fälligkeit.
import { Timer } from "lucide-react";
import { callInfo } from "../state/callTimer";
import type { CallTimerConfig } from "../types";

const TONE_CLS: Record<string, string> = {
  ok: "bg-emerald-100 text-emerald-800",
  warn: "bg-amber-100 text-amber-900",
  due: "bg-rose-100 text-rose-800",
};

export function CallTimerBadge({
  sinceMs,
  now,
  cfg,
}: {
  sinceMs: number;
  now: number;
  cfg: CallTimerConfig;
}) {
  const info = callInfo(sinceMs, now, cfg);
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs
                  font-semibold tabular-nums ${TONE_CLS[info.tone]}`}
      title={`Auf dem Feld seit ${info.clock} (m:ss) – ${info.label}`}
    >
      <Timer size={12} /> {info.clock} · {info.label}
    </span>
  );
}
