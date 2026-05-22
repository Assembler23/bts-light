import { useEffect, useState } from "react";
import {
  FolderOpen,
  ListOrdered,
  type LucideIcon,
  Monitor,
  Play,
  Radio,
  RefreshCw,
  SlidersHorizontal,
  Square,
  Tablet,
  Tv,
} from "lucide-react";
import { getStatus, openLiveView, openLogDir, startSync, stopSync } from "../api";
import { useUpdate } from "../components/UpdateBanner";
import type { AppConfig, SyncStatus } from "../types";

interface Props {
  config: AppConfig;
  onReconfigure: () => void;
  onOpenTablets: () => void;
  onOpenMonitors: () => void;
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
  variant?: "default" | "start" | "stop";
}) {
  const variant = props.variant ?? "default";
  const styles = {
    default: "bg-slate-100 text-slate-700 hover:bg-slate-200",
    start: "bg-emerald-600 text-white hover:bg-emerald-700",
    stop: "bg-rose-600 text-white hover:bg-rose-700",
  }[variant];
  const Icon = props.icon;
  return (
    <button
      onClick={props.onClick}
      disabled={props.disabled}
      title={props.title}
      className={`inline-flex items-center gap-2 rounded-lg px-3.5 py-2 text-sm
                  font-medium transition-colors disabled:opacity-50 ${styles}`}
    >
      <Icon size={16} strokeWidth={2} />
      {props.label}
    </button>
  );
}

export function Dashboard({
  config,
  onReconfigure,
  onOpenTablets,
  onOpenMonitors,
}: Props) {
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const { phase: updatePhase, checkNow } = useUpdate();
  const [updateChecked, setUpdateChecked] = useState(false);

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

  const updateMsg = updateMessage();
  const cloudMode = config.connection_mode === "cloud";

  return (
    <main className="mx-auto flex min-h-full max-w-xl flex-col gap-5 p-6 text-slate-800">
      <header className="flex items-center gap-3">
        <div className="flex h-11 w-11 items-center justify-center rounded-xl bg-slate-800 text-lg">
          🏸
        </div>
        <div>
          <h1 className="text-2xl font-semibold leading-tight">BTS Light</h1>
          <p className="text-sm text-slate-500">Liveticker-Status</p>
        </div>
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
              title="Öffentliche Liveticker-Seite im Browser öffnen"
            />
            <ActionButton
              icon={Monitor}
              label="Hallen-Monitor"
              onClick={() => openLiveView("monitor")}
              title="Großbild-Ansicht für einen Hallen-Monitor"
            />
            <ActionButton
              icon={ListOrdered}
              label="Nächste Spiele"
              onClick={() => openLiveView("next")}
              title="Aufruf-Anzeige der als Nächstes anstehenden Spiele"
            />
          </div>
        </section>
      )}

      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-slate-700">Steuerung</h2>
        <div className="flex flex-wrap gap-2.5">
          <ActionButton
            icon={status.running ? Square : Play}
            label={status.running ? "Stoppen" : "Starten"}
            onClick={toggle}
            disabled={busy}
            variant={status.running ? "stop" : "start"}
            title={
              status.running
                ? "Liveticker und Tablet-Server anhalten"
                : "Liveticker starten – BTP wird verbunden"
            }
          />
          <ActionButton
            icon={SlidersHorizontal}
            label="Einstellungen"
            onClick={onReconfigure}
            title="BTP-Verbindung, Verband und Tablet-Verbindungsart ändern"
          />
          <ActionButton
            icon={Tablet}
            label="Tablet-Spielzettel"
            onClick={onOpenTablets}
            title={
              cloudMode
                ? "Tablet-Adressen und Felder-Übersicht (Cloud-Modus)"
                : "Tablet-Adressen und Felder-Übersicht. Bekommen die Tablets " +
                  "im Hallen-WLAN keine Verbindung (z. B. Firewall)? Dann in " +
                  "den Einstellungen auf den Cloud-Modus umstellen."
            }
          />
          <ActionButton
            icon={Tv}
            label="Court-Monitore"
            onClick={onOpenMonitors}
            title="TV-Anzeigen am Spielfeld einrichten, zuweisen und steuern"
          />
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
