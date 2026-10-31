import { VolumeX } from "lucide-react";
import { useAusgabeProblem } from "../state/ansageAusgabe";
import type { TonBanner } from "../io/tonWeg.mjs";

const TEXT: Record<TonBanner, string> = {
  kein_geraet:
    "Kein Ausgabegerät gefunden – Ansage über das Programmfenster versucht.",
  keine_stimme:
    "Keine passende Windows-Stimme für die Ansage-Sprache installiert – Sprachpaket in den Windows-Einstellungen nachinstallieren. Bis dahin spricht die Browser-Stimme.",
  rueckfall:
    "Ansage über das Programmfenster (Rückfall) – die Ausgabe über Windows hat nicht geklappt.",
};

/**
 * App-weiter Hinweis, wenn die Ansage nicht über den Kern lief (Spec
 * `ansage-ausgabegeraet`). Kein Quittieren: Er verschwindet von selbst,
 * sobald die nächste Ansage wieder normal läuft.
 */
export function AnsageAusgabeBanner() {
  const art = useAusgabeProblem();
  if (!art) return null;
  return (
    <div className="flex items-center gap-2 bg-amber-500 px-4 py-2 text-sm font-medium text-white">
      <VolumeX size={16} className="shrink-0" />
      <span className="min-w-0">{TEXT[art]}</span>
    </div>
  );
}
