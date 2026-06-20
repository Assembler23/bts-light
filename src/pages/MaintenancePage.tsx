// Wartung: bisher ein Abschnitt auf dem Dashboard, jetzt ein eigener
// Menüpunkt unter den Einstellungen. Bündelt Update-Prüfung, Logs und die
// Versionsanzeige – alles, was den Betrieb der App selbst betrifft.
import { useEffect, useState } from "react";
import { FolderOpen, type LucideIcon, RefreshCw } from "lucide-react";
import { appVersion, openLogDir } from "../api";
import { useUpdate } from "../components/UpdateBanner";

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

export function MaintenancePage() {
  const { phase: updatePhase, checkNow } = useUpdate();
  const [updateChecked, setUpdateChecked] = useState(false);
  const [version, setVersion] = useState("");

  useEffect(() => {
    let active = true;
    appVersion()
      .then((v) => {
        if (active) setVersion(v);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, []);

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

  const updateMsg = updateMessage();

  return (
    <main className="mx-auto flex min-h-full max-w-xl flex-col gap-5 p-6 text-slate-800">
      <header>
        <h1 className="text-2xl font-semibold leading-tight">Wartung</h1>
        <p className="text-sm text-slate-500">
          Updates, Diagnose-Logs und Version
        </p>
      </header>

      <section className="flex flex-col gap-3 rounded-xl border border-slate-200 bg-white p-5 shadow-sm">
        <h2 className="text-sm font-semibold text-slate-700">Updates & Logs</h2>
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
          <p className="text-xs text-slate-500">{updateMsg}</p>
        )}
        {version !== "" && (
          <p className="text-xs text-slate-400">Version {version}</p>
        )}
      </section>
    </main>
  );
}
