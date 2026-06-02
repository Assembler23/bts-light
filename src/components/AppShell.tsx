// App-Rahmen: persistente Kopfzeile (Verband, Live-Status, Start/Stopp) plus
// die Seitenleiste. Der gewählte Bereich wird als `children` im Inhaltsbereich
// gerendert – ohne Zurück-Button, denn die Navigation ist immer sichtbar.
import { type ReactNode } from "react";
import { Play, Square } from "lucide-react";
import { tenantShortLabel } from "../presets";
import type { AppConfig, SyncStatus } from "../types";
import { type NavView, type SettingsFocus, SideNav } from "./SideNav";

function dotColor(status: SyncStatus | null): string {
  if (!status || !status.running) return "bg-slate-400";
  if (status.kind === "ok") return "bg-emerald-500";
  if (status.kind === "idle") return "bg-amber-400";
  return "bg-rose-500";
}

export function AppShell({
  current,
  config,
  status,
  busy,
  onToggleRun,
  onNavigate,
  children,
}: {
  current: NavView;
  config: AppConfig;
  status: SyncStatus | null;
  busy: boolean;
  onToggleRun: () => void;
  onNavigate: (view: NavView, focus?: SettingsFocus) => void;
  children: ReactNode;
}) {
  const running = status?.running ?? false;
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Kopfzeile – auf jeder Seite gleich. */}
      <header className="flex shrink-0 items-center gap-3 border-b border-slate-200 bg-white px-4 py-2.5">
        <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-slate-800 text-base">
          🏸
        </div>
        <div className="min-w-0">
          <div className="text-sm font-semibold leading-tight text-slate-800">
            BTS Light
          </div>
          <div className="truncate text-xs text-slate-500">
            {tenantShortLabel(config.badhub)}
          </div>
        </div>

        {/* Live-Status-Punkt mittig. */}
        <div className="ml-4 flex items-center gap-2">
          <span
            className={`h-2.5 w-2.5 rounded-full ${dotColor(status)} ${
              running && status?.kind === "ok" ? "animate-pulse" : ""
            }`}
          />
          <span className="text-sm text-slate-600">
            {running ? "Liveticker aktiv" : "Gestoppt"}
          </span>
        </div>

        {/* Start/Stopp – von überall erreichbar. */}
        <button
          onClick={onToggleRun}
          disabled={busy || !status}
          title={
            running
              ? "Liveticker und Tablet-Server anhalten"
              : "Liveticker starten – BTP wird verbunden"
          }
          className={`ml-auto inline-flex items-center gap-2 rounded-lg px-3.5 py-1.5 text-sm
                      font-medium text-white transition-colors disabled:opacity-50 ${
                        running
                          ? "bg-rose-600 hover:bg-rose-700"
                          : "bg-emerald-600 hover:bg-emerald-700"
                      }`}
        >
          {running ? <Square size={15} /> : <Play size={15} />}
          {running ? "Stoppen" : "Starten"}
        </button>
      </header>

      {/* Seitenleiste + Inhalt. */}
      <div className="flex min-h-0 flex-1">
        <SideNav current={current} config={config} onNavigate={onNavigate} />
        <div className="min-h-0 flex-1 overflow-auto">{children}</div>
      </div>
    </div>
  );
}
