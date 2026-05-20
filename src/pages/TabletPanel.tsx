import { useEffect, useState } from "react";
import { tabletOverview } from "../api";
import type { CourtOverview, TabletInfo } from "../types";

interface Props {
  onBack: () => void;
}

/**
 * Tablet-Spielzettel-Seite: oben die Adressen/QR-Codes zum Einrichten der
 * Tablets, darunter die Live-Felder-Übersicht für die Turnierleitung.
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
    <main className="mx-auto flex min-h-full max-w-xl flex-col gap-6 p-6 text-slate-800">
      <header className="flex items-center gap-3">
        <button
          onClick={onBack}
          className="rounded-lg bg-slate-200 px-3 py-1.5 text-sm"
        >
          ← Zurück
        </button>
        <div>
          <h1 className="text-2xl font-semibold">Tablet-Spielzettel</h1>
          <p className="text-sm text-slate-500">Einrichtung &amp; Felder-Übersicht</p>
        </div>
      </header>

      {courts.length === 0 ? (
        <p className="rounded-xl border border-slate-200 p-5 text-sm text-slate-500">
          Noch keine Spielfelder geladen. Starte den Liveticker (BTP muss
          verbunden sein) – danach erscheinen hier die Tablet-Adressen.
        </p>
      ) : (
        <>
          <section className="flex flex-col gap-2">
            <h2 className="text-sm font-semibold text-slate-700">
              Tablet-Adressen
            </h2>
            <p className="text-xs text-slate-500">
              Am Spielfeld diese Adresse im Browser öffnen oder den QR-Code
              scannen. Tablet und dieser PC müssen im selben WLAN sein.
            </p>
            <div className="flex flex-col gap-3">
              {courts.map((c) => {
                const path = encodeURIComponent(c.court);
                return (
                  <div
                    key={c.court}
                    className="flex items-center gap-3 rounded-lg border border-slate-200 p-3"
                  >
                    <img
                      src={`http://${host}/qr/${path}`}
                      alt=""
                      width={72}
                      height={72}
                      className="shrink-0 rounded bg-white"
                    />
                    <div className="min-w-0">
                      <div className="font-medium">{c.court}</div>
                      <div className="truncate text-xs text-slate-500">
                        {`http://${host}/court/${path}`}
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
            <div className="flex flex-col gap-2">
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
      <div className="flex items-center justify-between">
        <span className="font-medium">{court.court}</span>
        <span className="flex items-center gap-1.5 text-xs text-slate-500">
          <span
            className={`h-2 w-2 rounded-full ${
              court.tablet_connected ? "bg-green-500" : "bg-slate-300"
            }`}
          />
          {court.tablet_connected ? "Tablet verbunden" : "kein Tablet"}
        </span>
      </div>
      {hasMatch ? (
        <>
          {court.match_name !== "" && (
            <div className="mt-0.5 text-xs text-slate-500">
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
