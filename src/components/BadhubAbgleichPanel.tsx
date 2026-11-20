import { useEffect, useState } from "react";
import { CloudUpload, TriangleAlert } from "lucide-react";
import {
  badhubAbgleich,
  badhubSlaveSchluessel,
  badhubSteuerungUebernehmen,
  badhubTurnierBeenden,
} from "../api";
import type { BadhubAbgleich } from "../types";

/**
 * Abgleich mit badhub bei einem badhub-Turnier (Paket L3): Lage der Outbox
 * („badhub: aktuell" / „5 Ereignisse warten, letzter Kontakt 10:47"), roter
 * Hinweis, wenn die Steuerung in badhub zurückgeholt wurde, und die Knöpfe
 * „Turnier beenden / Steuerung an badhub zurückgeben" und „Steuerung wieder
 * übernehmen". Rendert nichts bei BTP-Turnieren (der Kern liefert `null`).
 */
export function BadhubAbgleichPanel() {
  const [lage, setLage] = useState<BadhubAbgleich | null>(null);
  const [laeuft, setLaeuft] = useState(false);
  const [meldung, setMeldung] = useState<{ ok: boolean; text: string } | null>(null);
  const [slaveSchluessel, setSlaveSchluessel] = useState<string | null>(null);

  useEffect(() => {
    badhubSlaveSchluessel().then(setSlaveSchluessel).catch(() => {});
  }, []);

  useEffect(() => {
    let aus = false;
    const holen = () =>
      badhubAbgleich()
        .then((l) => {
          if (!aus) setLage(l);
        })
        .catch(() => {});
    holen();
    const t = window.setInterval(holen, 3000);
    return () => {
      aus = true;
      window.clearInterval(t);
    };
  }, []);

  if (!lage) return null;

  const ausfuehren = async (f: () => Promise<string>) => {
    setLaeuft(true);
    setMeldung(null);
    try {
      setMeldung({ ok: true, text: await f() });
    } catch (e) {
      setMeldung({ ok: false, text: String(e) });
    } finally {
      setLaeuft(false);
      badhubAbgleich().then(setLage).catch(() => {});
    }
  };

  return (
    <section
      className={`flex flex-col gap-2 rounded-xl border p-4 ${
        lage.rot ? "border-rose-300 bg-rose-50" : "border-slate-200 bg-white"
      }`}
    >
      <div className="flex items-center gap-2 text-sm font-medium">
        {lage.rot ? (
          <TriangleAlert size={16} className="text-rose-600" />
        ) : (
          <CloudUpload size={16} className="text-slate-500" />
        )}
        <span className={lage.rot ? "text-rose-800" : "text-slate-700"}>{lage.text}</span>
      </div>
      <div className="flex flex-wrap gap-2">
        {lage.zustand !== "zurueckgegeben" && lage.zustand !== "entzogen" && (
          <button
            onClick={() => {
              if (
                window.confirm(
                  "Turnier beenden und die Steuerung an badhub zurückgeben? Danach sind Eingaben nur noch in badhub möglich, und die Namen werden von diesem PC gelöscht."
                )
              )
                void ausfuehren(badhubTurnierBeenden);
            }}
            disabled={laeuft || !lage.kann_beenden}
            title={
              lage.kann_beenden
                ? undefined
                : "Erst wenn alle Ereignisse bei badhub angekommen sind (badhub: aktuell)."
            }
            className="rounded-lg bg-slate-100 px-3.5 py-1.5 text-sm font-medium text-slate-700
                       transition-colors hover:bg-slate-200 disabled:opacity-50"
          >
            Turnier beenden / Steuerung an badhub zurückgeben
          </button>
        )}
        {lage.kann_uebernehmen && (
          <button
            onClick={() => {
              if (
                window.confirm(
                  "Steuerung wieder übernehmen? Der Stand wird aus badhub geladen; Eingaben dieses PCs, die nicht in badhub angekommen sind, werden verworfen."
                )
              )
                void ausfuehren(badhubSteuerungUebernehmen);
            }}
            disabled={laeuft}
            className="rounded-lg bg-slate-100 px-3.5 py-1.5 text-sm font-medium text-slate-700
                       transition-colors hover:bg-slate-200 disabled:opacity-50"
          >
            Steuerung wieder übernehmen
          </button>
        )}
      </div>
      {slaveSchluessel && (
        <p className="text-xs text-slate-500">
          Slave-Schlüssel für einen Ansage-Slave im Hallennetz:{" "}
          <code className="select-all font-mono">{slaveSchluessel}</code>
        </p>
      )}
      {meldung && (
        <p className={`text-sm ${meldung.ok ? "text-emerald-700" : "text-rose-700"}`}>
          {meldung.text}
        </p>
      )}
    </section>
  );
}
