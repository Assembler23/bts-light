import { useEffect, useState } from "react";
import { tabletOverview } from "../api";
import type { CourtOverview, TabletInfo } from "../types";

interface Props {
  onBack: () => void;
}

/**
 * Tablet-Spielzettel-Seite: oben die Adressen/QR-Codes zum Einrichten der
 * Tablets, darunter die Live-Felder-Übersicht für die Turnierleitung.
 * Beide Bereiche sind Raster – sie skalieren bis zu 20–30 Spielfeldern.
 * Pollt den Tablet-Server alle 2 s.
 */
export function TabletPanel({ onBack }: Props) {
  const [info, setInfo] = useState<TabletInfo | null>(null);

  useEffect(() => {
    let active = true;
    const tick = () => {
      tabletOverview()
        .then((i) => {
          if (active) setInfo(i);
        })
        .catch(() => {});
    };
    tick();
    const id = setInterval(tick, 2000);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, []);

  const host = info?.server_host ?? "";
  const courts = info?.courts ?? [];

  return (
    <main className="mx-auto flex min-h-full max-w-4xl flex-col gap-6 p-6 text-slate-800">
      <header className="flex items-center gap-3">
        <button
          onClick={onBack}
          className="rounded-lg bg-slate-200 px-3 py-1.5 text-sm"
        >
          ← Zurück
        </button>
        <div className="flex-1">
          <h1 className="text-2xl font-semibold">Tablet-Spielzettel</h1>
          <p className="text-sm text-slate-500">
            {courts.length > 0
              ? `${courts.length} Spielfelder · Server ${host}`
              : "Einrichtung & Felder-Übersicht"}
          </p>
        </div>
      </header>

      {courts.length === 0 ? (
        <p className="rounded-xl border border-slate-200 p-5 text-sm text-slate-500">
          Noch keine Spielfelder geladen. Starte den Liveticker (BTP muss
          verbunden sein) – danach erscheinen hier die Tablet-Adressen für
          alle Felder. Die Zahl der Tablets ist nicht begrenzt.
        </p>
      ) : (
        <>
          <section className="flex flex-col gap-2">
            <h2 className="text-sm font-semibold text-slate-700">
              Tablet-Adressen
            </h2>
            <p className="text-xs text-slate-500">
              Am Spielfeld diese Adresse im Browser öffnen oder den QR-Code
              scannen. Jedes Feld bekommt ein eigenes Tablet; Tablet und
              dieser PC müssen im selben WLAN sein.
            </p>
            <div className="grid grid-cols-2 gap-2 md:grid-cols-3">
              {courts.map((c) => {
                const path = encodeURIComponent(c.court);
                return (
                  <div
                    key={c.court}
                    className="flex items-center gap-2 rounded-lg border border-slate-200 p-2"
                  >
                    <img
                      src={`http://${host}/qr/${path}`}
                      alt=""
                      width={56}
                      height={56}
                      className="shrink-0 rounded bg-white"
                    />
                    <div className="min-w-0">
                      <div className="truncate text-sm font-medium">
                        {c.court}
                      </div>
                      <div className="truncate text-[11px] text-slate-400">
                        /court/{c.court}
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          </section>

          <section className="flex flex-col gap-2">
            <h2 className="text-sm font-semibold text-slate-700">
              Felder-Übersicht
            </h2>
            <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
              {courts.map((c) => (
                <CourtCard key={c.court} court={c} />
              ))}
            </div>
          </section>
        </>
      )}
    </main>
  );
}

function CourtCard({ court }: { court: CourtOverview }) {
  const team1 = court.team1.join(" / ") || "—";
  const team2 = court.team2.join(" / ") || "—";
  const hasMatch = court.match_name !== "" || court.team1.length > 0;

  return (
    <div className="rounded-lg border border-slate-200 p-3">
      <div className="flex items-center justify-between gap-2">
        <span className="truncate font-medium">{court.court}</span>
        <span className="flex shrink-0 items-center gap-1.5 text-xs text-slate-500">
          <span
            className={`h-2 w-2 rounded-full ${
              court.tablet_connected ? "bg-green-500" : "bg-slate-300"
            }`}
          />
          {court.tablet_connected ? "Tablet" : "kein Tablet"}
        </span>
      </div>
      {hasMatch ? (
        <>
          {court.match_name !== "" && (
            <div className="mt-0.5 truncate text-xs text-slate-500">
              {court.match_name}
            </div>
          )}
          <div className="mt-1 flex justify-between gap-3 text-sm">
            <span className="min-w-0 truncate">{team1}</span>
            <span className="shrink-0 font-mono font-semibold tabular-nums">
              {court.sets.map((s) => s[0]).join("  ")}
            </span>
          </div>
          <div className="flex justify-between gap-3 text-sm">
            <span className="min-w-0 truncate">{team2}</span>
            <span className="shrink-0 font-mono font-semibold tabular-nums">
              {court.sets.map((s) => s[1]).join("  ")}
            </span>
          </div>
        </>
      ) : (
        <div className="mt-1 text-sm text-slate-400">frei</div>
      )}
    </div>
  );
}
