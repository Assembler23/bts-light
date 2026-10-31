import { VolumeX } from "lucide-react";
import { useEffect, useState } from "react";
import { ansageAusgabe, type AusgabeInfo } from "../api";
import {
  loeseGeraetFehlt,
  useAusgabeProbleme,
} from "../state/ansageAusgabe";
import type { TonBanner } from "../io/tonWeg.mjs";

const TEXT: Record<Exclude<TonBanner, "geraet_fehlt">, string> = {
  kein_geraet:
    "Kein Ausgabegerät gefunden – Ansage über das Programmfenster versucht.",
  keine_stimme:
    "Keine passende Windows-Stimme für die Ansage-Sprache installiert – Sprachpaket in den Windows-Einstellungen nachinstallieren. Bis dahin spricht die Browser-Stimme.",
  rueckfall:
    "Ansage über das Programmfenster (Rückfall) – die Ausgabe über Windows hat nicht geklappt.",
};

/** Abfrage, solange „Gerät fehlt" steht (Spec ansage-ausgabegeraet, PR 2). */
const GERAET_TAKT_MS = 5000;

function geraetFehltText(info: AusgabeInfo | null): string {
  const name = info?.gewaehlt || "Gewähltes Ausgabegerät";
  const standard = info?.standard ? ` (${info.standard})` : "";
  return `${name} nicht verfügbar – Ansage läuft über den Windows-Standard${standard}.`;
}

/**
 * App-weite Hinweise, wenn die Ansage nicht wie gewählt lief (Spec
 * `ansage-ausgabegeraet`). Kein Quittieren: Sie verschwinden von selbst,
 * sobald die Ursache weg ist.
 */
export function AnsageAusgabeBanner() {
  const probleme = useAusgabeProbleme();
  const fehlt = probleme.includes("geraet_fehlt");
  const [info, setInfo] = useState<AusgabeInfo | null>(null);

  // „Gerät fehlt": nachfragen, bis es zurück ist (oder die Wahl auf den
  // Windows-Standard gestellt wurde) — dann verschwindet der Hinweis, und
  // die nächste Ansage läuft wieder darüber.
  useEffect(() => {
    if (!fehlt) return;
    let aktiv = true;
    let laeuft = false;
    const pruefen = () => {
      if (laeuft) return;
      laeuft = true;
      ansageAusgabe()
        .then((i) => {
          if (!aktiv) return;
          setInfo(i);
          if (i.verfuegbar) loeseGeraetFehlt();
        })
        .catch(() => {})
        .finally(() => {
          laeuft = false;
        });
    };
    pruefen();
    const t = window.setInterval(pruefen, GERAET_TAKT_MS);
    return () => {
      aktiv = false;
      window.clearInterval(t);
    };
  }, [fehlt]);

  if (probleme.length === 0) return null;
  return (
    <div className="flex flex-col gap-1 bg-amber-500 px-4 py-2 text-sm font-medium text-white">
      {probleme.map((art) => (
        <span key={art} className="flex min-w-0 items-center gap-2">
          <VolumeX size={16} className="shrink-0" />
          {art === "geraet_fehlt" ? geraetFehltText(info) : TEXT[art]}
        </span>
      ))}
    </div>
  );
}
