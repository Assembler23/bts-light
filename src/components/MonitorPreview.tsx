interface MonitorPreviewProps {
  showDiscipline: boolean;
  showRound: boolean;
  showMatchNumber: boolean;
  showTimer: boolean;
  showMatchClock: boolean;
}

/**
 * Verkleinerte Live-Vorschau der Court-Monitor-Anzeige („Arena"-Design).
 * Spiegelt die Anzeige-Optionen wider, damit man beim Setzen der Häkchen
 * direkt sieht, wie der TV später aussieht. Beispieldaten sind fest – es
 * geht nur um die Wirkung der Optionen.
 */
export function MonitorPreview({
  showDiscipline,
  showRound,
  showMatchNumber,
  showTimer,
  showMatchClock,
}: MonitorPreviewProps) {
  const footer = [
    showRound ? "Gruppe 2" : null,
    showMatchNumber ? "Spiel 14" : null,
  ]
    .filter(Boolean)
    .join("  ·  ");

  return (
    <div
      className="select-none overflow-hidden rounded-lg border border-slate-700
                 bg-slate-950 shadow-inner"
    >
      {/* Kopfzeile */}
      <div className="flex items-center gap-1.5 border-b-2 border-amber-400 bg-slate-800 px-2.5 py-1.5">
        <span className="h-1.5 w-1.5 rounded-full bg-amber-400" />
        <span className="text-[11px] font-extrabold uppercase tracking-wider text-amber-400">
          Feld 3
        </span>
        {showMatchClock && (
          <span className="text-[10px] font-bold tabular-nums text-slate-400">
            ⏱ 12 min
          </span>
        )}
        {showDiscipline && (
          <span className="ml-auto text-[10px] font-semibold text-slate-300">
            Herreneinzel
          </span>
        )}
      </div>

      <div className="relative">
        {/* Mannschaft 1 – schlägt auf (Amber-Akzent) */}
        <div className="flex items-center gap-2 border-l-2 border-amber-400 bg-amber-400/[.06] px-2.5 py-2.5">
          <span className="text-base leading-none">🇩🇪</span>
          <span className="min-w-0 flex-1 leading-tight">
            <span className="block text-[7px] font-semibold uppercase tracking-wide text-slate-400">
              Anna
            </span>
            <span className="block truncate text-xs font-bold text-slate-100">
              Müller
            </span>
          </span>
          <span className="text-[9px] text-amber-400">●</span>
          <span className="text-[10px] font-bold text-slate-500">21</span>
          <span className="rounded bg-amber-400 px-1.5 py-0.5 text-xs font-extrabold text-slate-900">
            11
          </span>
        </div>
        <div className="h-px bg-slate-700" />
        {/* Mannschaft 2 */}
        <div className="flex items-center gap-2 border-l-2 border-transparent px-2.5 py-2.5">
          <span className="text-base leading-none">🇵🇱</span>
          <span className="min-w-0 flex-1 leading-tight">
            <span className="block text-[7px] font-semibold uppercase tracking-wide text-slate-400">
              Hilde
            </span>
            <span className="block truncate text-xs font-bold text-slate-100">
              Kowalski
            </span>
          </span>
          <span className="text-[10px] font-bold text-slate-500">18</span>
          <span className="rounded border border-slate-600 bg-slate-900 px-1.5 py-0.5 text-xs font-extrabold text-slate-100">
            7
          </span>
        </div>

        {/* Pausen-Countdown (Retro-Klappanzeige), oben rechts angedeutet */}
        {showTimer && (
          <div className="absolute right-2 top-2 flex gap-0.5">
            {["1", ":", "2", "0"].map((ch, i) => (
              <span
                key={i}
                className={
                  ch === ":"
                    ? "text-xs font-extrabold text-slate-300"
                    : "rounded bg-slate-800 px-1 text-xs font-extrabold text-slate-100 shadow"
                }
              >
                {ch}
              </span>
            ))}
          </div>
        )}
      </div>

      {/* Fußzeile */}
      {footer && (
        <div className="bg-slate-800 px-2.5 py-1 text-right text-[10px] font-semibold text-slate-400">
          {footer}
        </div>
      )}
    </div>
  );
}
