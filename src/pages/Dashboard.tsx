import { useEffect, useState } from "react";
import { getStatus, openLiveView, startSync, stopSync } from "../api";
import type { AppConfig, SyncStatus } from "../types";

interface Props {
  config: AppConfig;
  onReconfigure: () => void;
}

function dotColor(status: SyncStatus): string {
  if (!status.running) return "bg-slate-400";
  if (status.kind === "ok") return "bg-green-500";
  if (status.kind === "idle") return "bg-amber-400";
  return "bg-red-500";
}

function ago(ms: number): string {
  if (ms === 0) return "–";
  const secs = Math.max(0, Math.round((Date.now() - ms) / 1000));
  if (secs < 60) return `vor ${secs} s`;
  return `vor ${Math.round(secs / 60)} min`;
}

export function Dashboard({ config, onReconfigure }: Props) {
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let active = true;
    const tick = () => {
      getStatus()
        .then((s) => {
          if (active) setStatus(s);
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

  async function toggle() {
    if (!status) return;
    setBusy(true);
    try {
      if (status.running) {
        await stopSync();
      } else {
        await startSync();
      }
      setStatus(await getStatus());
    } finally {
      setBusy(false);
    }
  }

  if (!status) {
    return (
      <main className="flex h-full items-center justify-center text-slate-400">
        Lädt …
      </main>
    );
  }

  return (
    <main className="mx-auto flex min-h-full max-w-xl flex-col gap-6 p-6 text-slate-800">
      <header>
        <h1 className="text-2xl font-semibold">BTS Light</h1>
        <p className="text-sm text-slate-500">Liveticker-Status</p>
      </header>

      <section className="rounded-xl border border-slate-200 p-5">
        <div className="flex items-center gap-3">
          <span className={`h-3 w-3 rounded-full ${dotColor(status)}`} />
          <span className="font-medium">
            {status.running ? "Liveticker aktiv" : "Gestoppt"}
          </span>
        </div>
        <p className="mt-2 text-sm text-slate-600">{status.message}</p>
        <p className="mt-1 text-xs text-slate-400">
          Letzter Stand: {ago(status.updated_at_ms)}
        </p>
      </section>

      {config.badhub.live_url !== "" && (
        <section className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-slate-700">
            Anzeigen im Browser öffnen
          </h2>
          <div className="flex flex-wrap gap-2">
            <button
              onClick={() => openLiveView(null)}
              className="rounded-lg bg-slate-200 px-3 py-1.5 text-sm"
            >
              Liveticker
            </button>
            <button
              onClick={() => openLiveView("monitor")}
              className="rounded-lg bg-slate-200 px-3 py-1.5 text-sm"
            >
              Hallen-Monitor
            </button>
            <button
              onClick={() => openLiveView("next")}
              className="rounded-lg bg-slate-200 px-3 py-1.5 text-sm"
            >
              Nächste Spiele
            </button>
          </div>
        </section>
      )}

      <div className="flex gap-3">
        <button
          onClick={toggle}
          disabled={busy}
          className="rounded-lg bg-slate-800 px-4 py-2 text-sm font-medium text-white
                     disabled:opacity-50"
        >
          {status.running ? "Stoppen" : "Starten"}
        </button>
        <button
          onClick={onReconfigure}
          className="rounded-lg bg-slate-200 px-4 py-2 text-sm"
        >
          Einstellungen ändern
        </button>
      </div>
    </main>
  );
}
