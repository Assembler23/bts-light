import { VolumeX, X } from "lucide-react";
import { dismissAzureFallback, useAzureFallback } from "../state/azureStatus";

/**
 * App-weiter Hinweis, wenn eine Azure-Ansage fehlgeschlagen ist und die
 * Web-Speech-Standardstimme übernommen hat. Vorher passierte dieser Rückfall
 * stumm — beim Zwei-Hallen-Test fiel deshalb erst in der Halle auf, dass am
 * Slave der Azure-Key fehlte. Bleibt stehen, bis er quittiert wird (oder ein
 * neuer Fehler ihn aktualisiert).
 */
export function AzureFallbackBanner() {
  const info = useAzureFallback();
  if (!info) return null;

  return (
    <div className="flex items-center gap-2 bg-amber-500 px-4 py-2 text-sm font-medium text-white">
      <VolumeX size={16} className="shrink-0" />
      <span className="min-w-0">
        Hochwertige Azure-Ansage fehlgeschlagen — die Standardstimme übernimmt.
        <span className="ml-1 font-normal opacity-90">({info.message})</span>
      </span>
      <button
        type="button"
        title="Hinweis ausblenden"
        onClick={dismissAzureFallback}
        className="ml-auto shrink-0 rounded p-0.5 transition-colors hover:bg-amber-600"
      >
        <X size={16} />
      </button>
    </div>
  );
}
