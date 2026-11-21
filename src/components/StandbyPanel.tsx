import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { LifeBuoy, TriangleAlert } from "lucide-react";
import { badhubSicherungEinlesen, badhubStandbyLage, badhubStandbyUebernehmen } from "../api";
import type { StandbyLage } from "../types";

/**
 * Standby-PC eines badhub-Turniers (Paket S2): Spiegelstand („Standby für
 * Halle 1 PC, gespiegelt bis n = 412, vor 3 s“), Notfall-Sicherung einlesen
 * und „Hauptrechner ausgefallen – Turnier übernehmen“. Rendert nichts bei
 * BTP oder ohne geladenes badhub-Turnier.
 */
export function StandbyPanel() {
  const [lage, setLage] = useState<StandbyLage | null>(null);
  const [laeuft, setLaeuft] = useState(false);
  const [meldung, setMeldung] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    let aus = false;
    const holen = () =>
      badhubStandbyLage()
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
      badhubStandbyLage().then(setLage).catch(() => {});
    }
  };

  const einlesen = async () => {
    const pfad = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Notfall-Sicherung", extensions: ["btsk"] }],
    });
    if (typeof pfad === "string") void ausfuehren(() => badhubSicherungEinlesen(pfad));
  };

  return (
    <section className="flex flex-col gap-2 rounded-xl border border-slate-200 bg-white p-4">
      <div className="flex items-center gap-2 text-sm font-medium text-slate-700">
        <LifeBuoy size={16} className="text-slate-500" />
        <span>{lage.text}</span>
      </div>
      {lage.hinweis && <p className="text-xs text-amber-700">{lage.hinweis}</p>}
      <div className="flex flex-wrap gap-2">
        <button
          onClick={() => void einlesen()}
          disabled={laeuft}
          className="rounded-lg bg-slate-100 px-3.5 py-1.5 text-sm font-medium text-slate-700
                     transition-colors hover:bg-slate-200 disabled:opacity-50"
        >
          Notfall-Sicherung einlesen
        </button>
        {lage.rolle === "standby" && (
          <button
            onClick={() => {
              if (
                window.confirm(
                  "Hauptrechner ausgefallen – Turnier übernehmen?\n\nNur, wenn der Hauptrechner wirklich aus ist! Schreiben beide PCs weiter, nimmt badhub nur die Eingaben der Seite an, die zuerst hochlädt; die der anderen werden abgewiesen.\n\nDanach ist dieser PC der Turnier-PC: Tablets und Monitore müssen neu gekoppelt werden."
                )
              )
                void ausfuehren(badhubStandbyUebernehmen);
            }}
            disabled={laeuft || !lage.kann_uebernehmen}
            className="flex items-center gap-1.5 rounded-lg bg-rose-600 px-3.5 py-1.5 text-sm
                       font-medium text-white transition-colors hover:bg-rose-700 disabled:opacity-50"
          >
            <TriangleAlert size={14} /> Hauptrechner ausgefallen – Turnier übernehmen
          </button>
        )}
      </div>
      {meldung && (
        <p className={`text-sm ${meldung.ok ? "text-emerald-700" : "text-rose-700"}`}>
          {meldung.text}
        </p>
      )}
    </section>
  );
}
