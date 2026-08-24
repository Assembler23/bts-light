import { useEffect, useState } from "react";
import { PrinterX, X } from "lucide-react";
import { clearPrintWarning, printWarning } from "../api";

/**
 * App-weiter Hinweis, wenn der Zettel-Autodruck gescheitert ist (Spec
 * `schiedsrichterzettel-autodruck`, E5).
 *
 * **Warum überhaupt sichtbar?** Ein Drucker, der aus ist, kein Papier hat
 * oder umbenannt wurde, scheitert sonst stumm — und die Turnierleitung
 * wartet an den Feldern auf Zettel, die nie kommen. Der Druck selbst
 * wiederholt sich bewusst nicht; deshalb ist diese Meldung der einzige
 * Weg, es zu merken.
 *
 * Steht app-weit statt in einem Tab: Die Feldvergabe läuft auch, während
 * jemand in den Einstellungen ist.
 */
export function PrintWarningBanner() {
  const [text, setText] = useState<string | null>(null);

  useEffect(() => {
    let aktiv = true;
    const holen = () => {
      printWarning()
        .then((w) => {
          if (aktiv) setText(w);
        })
        .catch(() => {});
    };
    holen();
    // Gemächlicher Takt: Die Meldung ist kein Livewert, und sie bleibt
    // stehen, bis jemand sie wegklickt.
    const id = window.setInterval(holen, 10_000);
    return () => {
      aktiv = false;
      window.clearInterval(id);
    };
  }, []);

  if (!text) return null;

  return (
    <div className="flex items-center gap-2 bg-amber-500 px-4 py-2 text-sm font-medium text-white">
      <PrinterX size={16} className="shrink-0" />
      <span className="min-w-0">
        Schiedsrichterzettel konnte nicht gedruckt werden.
        <span className="ml-1 font-normal opacity-90">({text})</span>
      </span>
      <button
        type="button"
        title="Hinweis ausblenden"
        onClick={() => {
          setText(null);
          clearPrintWarning().catch(() => {});
        }}
        className="ml-auto shrink-0 rounded p-0.5 transition-colors hover:bg-amber-600"
      >
        <X size={16} />
      </button>
    </div>
  );
}
