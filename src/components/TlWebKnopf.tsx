// Ein Klick zur Turnierleitungs-Oberfläche – am Turnier-PC selbst.
//
// Wer am Rechner sitzt, auf dem bts-light läuft, soll nicht erst ein Gerät
// koppeln, die Adresse kopieren und im Browser einfügen müssen. Der Knopf
// öffnet die Oberfläche im Standard-Browser; beim ersten Druck legt der Kern
// dafür den Zugang „Dieser PC" an (sichtbar und entziehbar in der
// Geräteliste unter „Turnierleitung").
//
// Ein zweiter Druck öffnet keinen zweiten Tab neben dem ersten: Die Seite
// selbst räumt auf (Marke `app=1`, siehe `src/io/einTab.mjs`) – von außen
// lässt sich ein offener Browser-Tab nicht ansteuern.
import { useEffect, useState } from "react";
import { ExternalLink, Loader2, X } from "lucide-react";
import { tlWebOpenLocal } from "../api";
import type { AppConfig } from "../types";

/** So lange bleibt eine Fehlermeldung stehen, wenn sie niemand wegklickt. */
const FEHLER_SICHTBAR_MS = 9000;

export function TlWebKnopf({
  label,
  running,
  kompakt = false,
  onConfigSaved,
}: {
  /** Beschriftung – in der Kopfzeile knapp, auf der Seite ausgeschrieben. */
  label: string;
  /** Läuft die Übertragung? Ohne sie gibt es die Oberfläche nicht. */
  running: boolean;
  /** Kopfzeile: In schmalen Fenstern bleibt nur das Sinnbild stehen. Gemessen
   *  am Mindestfenster (720 px): Mit Wort schob der Knopf „Stoppen" bis zu
   *  105 px aus dem Bild, mit Sinnbild allein passt alles. */
  kompakt?: boolean;
  /** Der erste Druck speichert einen Zugang. Die App hält eine eigene Kopie
   *  der Konfiguration – bliebe sie alt, löschte der nächste Speichervorgang
   *  aus den Einstellungen den Zugang wieder. */
  onConfigSaved: (c: AppConfig) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [fehler, setFehler] = useState<string | null>(null);

  useEffect(() => {
    if (!fehler) return;
    const t = window.setTimeout(() => setFehler(null), FEHLER_SICHTBAR_MS);
    return () => window.clearTimeout(t);
  }, [fehler]);

  async function oeffnen() {
    if (busy) return;
    setBusy(true);
    setFehler(null);
    try {
      const cfg = await tlWebOpenLocal();
      if (cfg) onConfigSaved(cfg);
    } catch (e) {
      // Der Kern sagt, woran es liegt (nicht gestartet, Geräteliste voll …).
      setFehler(typeof e === "string" ? e : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <span className="relative inline-flex shrink-0">
      {/* Bewusst nicht `disabled`, wenn die Übertragung steht: Ein toter
          Knopf erklärt sich nicht. So sagt der Klick, was fehlt. */}
      <button
        type="button"
        onClick={oeffnen}
        aria-busy={busy}
        title={
          running
            ? "Turnierleitungs-Oberfläche im Browser dieses PCs öffnen – ohne Koppeln"
            : "Turnierleitungs-Oberfläche – erst verfügbar, wenn bts-light gestartet ist"
        }
        className={`inline-flex items-center gap-2 whitespace-nowrap rounded-lg border px-3 py-1.5
                    text-sm font-medium transition-colors ${
                      running
                        ? "border-slate-300 bg-white text-slate-700 hover:bg-slate-50"
                        : "border-slate-200 bg-white text-slate-400 hover:bg-slate-50"
                    }`}
      >
        {busy ? (
          <Loader2 size={15} className="animate-spin" />
        ) : (
          <ExternalLink size={15} />
        )}
        {/* Für Vorlesehilfen bleibt das Wort auch dann, wenn es nicht zu
            sehen ist. */}
        <span
          className={kompakt ? "sr-only min-[880px]:not-sr-only" : undefined}
        >
          {label}
        </span>
      </button>
      {fehler && (
        // Die Meldung selbst trägt die Alarm-Rolle (Vorlesehilfen sagen sie
        // an); zum Wegklicken dient ein eigener Knopf – ein Knopf MIT
        // Alarm-Rolle wäre für Vorlesehilfen keiner mehr.
        <div
          role="alert"
          className="absolute right-0 top-full z-50 mt-2 flex w-72 items-start gap-2 rounded-lg
                     border border-rose-200 bg-rose-50 px-3 py-2 text-left text-xs
                     font-normal leading-snug text-rose-800 shadow-lg"
        >
          <span className="min-w-0 flex-1">{fehler}</span>
          <button
            type="button"
            onClick={() => setFehler(null)}
            aria-label="Meldung schließen"
            title="Meldung schließen"
            className="-mr-1 -mt-0.5 shrink-0 rounded p-0.5 text-rose-500 hover:bg-rose-100
                       hover:text-rose-800"
          >
            <X size={14} />
          </button>
        </div>
      )}
    </span>
  );
}
