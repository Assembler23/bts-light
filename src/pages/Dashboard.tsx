import { useEffect, useState } from "react";
import {
  FolderOpen,
  ListOrdered,
  type LucideIcon,
  Monitor,
  Radio,
  RefreshCw,
} from "lucide-react";
import { openExternal, openLiveView, openLogDir, tabletOverview } from "../api";
import { useUpdate } from "../components/UpdateBanner";
import type { AppConfig, SyncStatus } from "../types";

interface Props {
  config: AppConfig;
  /** Live-Status – kommt von App (geteilt mit der Kopfzeile). */
  status: SyncStatus | null;
}

function dotColor(status: SyncStatus): string {
  if (!status.running) return "bg-slate-400";
  if (status.kind === "ok") return "bg-emerald-500";
  if (status.kind === "idle") return "bg-amber-400";
  return "bg-rose-500";
}

function ago(ms: number): string {
  if (ms === 0) return "–";
  const secs = Math.max(0, Math.round((Date.now() - ms) / 1000));
  if (secs < 60) return `vor ${secs} s`;
  return `vor ${Math.round(secs / 60)} min`;
}

/** Einheitlicher Aktions-Button mit Icon, Beschriftung und Tooltip. */
function ActionButton(props: {
  icon: LucideIcon;
  label: string;
  onClick: () => void;
  disabled?: boolean;
  title?: string;
}) {
  const Icon = props.icon;
  return (
    <button
      onClick={props.onClick}
      disabled={props.disabled}
      title={props.title}
      className="inline-flex items-center gap-2 rounded-lg bg-slate-100 px-3.5 py-2 text-sm
                 font-medium text-slate-700 transition-colors hover:bg-slate-200
                 disabled:opacity-50"
    >
      <Icon size={16} strokeWidth={2} />
      {props.label}
    </button>
  );
}

export function Dashboard({ config, status }: Props) {
  const { phase: updatePhase, checkNow } = useUpdate();
  const [updateChecked, setUpdateChecked] = useState(false);
  // Hallen erst NACH dem Start bekannt (dann steht die BTP-Verbindung und
  // die Felder/Hallen aus der Turnierdatei). Bei ≥2 Hallen blenden wir je
  // Halle einen lokalen Hallen-Monitor-Button ein.
  const running = status?.running ?? false;
  const [halls, setHalls] = useState<string[]>([]);
  useEffect(() => {
    if (!running) {
      setHalls([]);
      return;
    }
    let active = true;
    const load = () => {
      tabletOverview()
        .then((info) => {
          if (!active) return;
          setHalls(
            [
              ...new Set(
                (info.courts ?? [])
                  .map((c) => c.location)
                  .filter((l) => l !== ""),
              ),
            ].sort((a, b) => a.localeCompare(b, "de")),
          );
        })
        .catch(() => {});
    };
    load();
    const id = setInterval(load, 20000);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, [running]);

  async function handleCheckUpdate() {
    setUpdateChecked(true);
    await checkNow();
  }

  // Rückmeldung nur nach einem manuellen Klick zeigen – der Auto-Check
  // beim Start soll hier keine Meldung hinterlassen.
  function updateMessage(): string {
    if (!updateChecked) return "";
    if (updatePhase === "checking") return "Prüfe auf Update …";
    if (updatePhase === "available") return "Update verfügbar – siehe Banner oben.";
    if (updatePhase === "current") return "Aktuell auf dem neuesten Stand.";
    if (updatePhase === "error") return "Update-Prüfung fehlgeschlagen (offline?).";
    return "";
  }

  if (!status) {
    return (
      <main className="flex h-full items-center justify-center text-slate-400">
        Lädt …
      </main>
    );
  }

  const updateMsg = updateMessage();

  return (
    <main className="mx-auto flex min-h-full max-w-xl flex-col gap-5 p-6 text-slate-800">
      <header>
        <h1 className="text-2xl font-semibold leading-tight">Status</h1>
        <p className="text-sm text-slate-500">Liveticker-Status</p>
      </header>

      <section className="rounded-xl border border-slate-200 bg-white p-5 shadow-sm">
        <div className="flex items-center gap-2.5">
          <span
            className={`h-3 w-3 rounded-full ${dotColor(status)} ${
              status.running && status.kind === "ok" ? "animate-pulse" : ""
            }`}
          />
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
            <ActionButton
              icon={Radio}
              label="Liveticker"
              onClick={() => openLiveView(null)}
              disabled={!running}
              title={
                running
                  ? "Öffentliche Liveticker-Seite im Browser öffnen"
                  : "Erst nach Start des Livetickers verfügbar"
              }
            />
            <ActionButton
              icon={Monitor}
              label="Hallen-Monitor"
              onClick={() => openLiveView("monitor")}
              disabled={!running}
              title={
                running
                  ? "Großbild-Ansicht für einen Hallen-Monitor (online)"
                  : "Erst nach Start des Livetickers verfügbar"
              }
            />
            <ActionButton
              icon={ListOrdered}
              label="Nächste Spiele"
              onClick={() => openLiveView("next")}
              disabled={!running}
              title={
                running
                  ? "Aufruf-Anzeige der als Nächstes anstehenden Spiele"
                  : "Erst nach Start des Livetickers verfügbar"
              }
            />
          </div>
          {!running && (
            <p className="text-xs text-slate-400">
              Verfügbar, sobald der Liveticker gestartet ist (dann steht die
              Verbindung zu BTP).
            </p>
          )}
          {/* Ab 2 Hallen je Halle ein lokaler Hallen-Monitor (Court-Übersicht
              dieser Halle) – am PC im Browser geöffnet. */}
          {running && halls.length >= 2 && (
            <div className="mt-1 flex flex-col gap-1.5">
              <span className="text-xs text-slate-500">
                Hallen-Monitor je Halle (lokal):
              </span>
              <div className="flex flex-wrap gap-2">
                {halls.map((hall) => (
                  <ActionButton
                    key={hall}
                    icon={Monitor}
                    label={hall}
                    onClick={() =>
                      void openExternal(
                        `http://localhost:8088/info/overview?halle=${encodeURIComponent(hall)}`,
                      )
                    }
                    title={`Lokale Court-Übersicht für ${hall} im Browser öffnen`}
                  />
                ))}
              </div>
            </div>
          )}
        </section>
      )}

      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-slate-700">Wartung</h2>
        <div className="flex flex-wrap gap-2.5">
          <ActionButton
            icon={RefreshCw}
            label="Nach Update prüfen"
            onClick={handleCheckUpdate}
            disabled={updatePhase === "checking"}
            title="Auf eine neue BTS-Light-Version prüfen"
          />
          <ActionButton
            icon={FolderOpen}
            label="Logs öffnen"
            onClick={() => void openLogDir()}
            title="Den Ordner mit den Diagnose-Logdateien öffnen"
          />
        </div>
        {updateMsg !== "" && (
          <p className="mt-1 text-xs text-slate-500">{updateMsg}</p>
        )}
      </section>
    </main>
  );
}
