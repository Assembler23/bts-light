// Wartung: bisher ein Abschnitt auf dem Dashboard, jetzt ein eigener
// Menüpunkt unter den Einstellungen. Bündelt Update-Prüfung, Logs und die
// Versionsanzeige – alles, was den Betrieb der App selbst betrifft.
import { useEffect, useRef, useState } from "react";
import {
  Download,
  FolderOpen,
  type LucideIcon,
  RefreshCw,
  Upload,
} from "lucide-react";
import { appVersion, exportIdentity, importIdentity, openLogDir } from "../api";
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
  // Identitäts-Umzug (ADR 0006): Statusmeldung + Datei für die Import-Bestätigung.
  const [idMsg, setIdMsg] = useState("");
  const [pendingImport, setPendingImport] = useState<string | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);

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
    if (updatePhase === "available")
      return "Update verfügbar – siehe Banner oben.";
    if (updatePhase === "current") return "Aktuell auf dem neuesten Stand.";
    if (updatePhase === "error")
      return "Update-Prüfung fehlgeschlagen (offline?).";
    return "";
  }

  const updateMsg = updateMessage();

  // Identität exportieren: JSON-Bündel als Datei herunterladen (WebView-
  // Download, kein Datei-Dialog-Plugin nötig).
  async function handleExportIdentity() {
    setIdMsg("");
    try {
      const json = await exportIdentity();
      const url = URL.createObjectURL(
        new Blob([json], { type: "application/json" }),
      );
      const a = document.createElement("a");
      a.href = url;
      a.download = "bts-light-identitaet.json";
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
      setIdMsg(
        "Identität exportiert. Datei sicher aufbewahren — sie enthält den Kopplungs-Token (wie ein Passwort behandeln).",
      );
    } catch (e) {
      setIdMsg(String(e));
    }
  }

  // Datei gewählt → Inhalt lesen und zur Bestätigung vormerken (Import
  // überschreibt die lokale Identität).
  async function handleImportFileChosen(file: File | undefined) {
    if (!file) return;
    setIdMsg("");
    try {
      setPendingImport(await file.text());
    } catch {
      setIdMsg("Datei konnte nicht gelesen werden.");
    }
  }

  async function confirmImport() {
    if (pendingImport == null) return;
    try {
      await importIdentity(pendingImport);
      setPendingImport(null);
      setIdMsg(
        "Identität importiert. Bitte bts-light neu starten, damit alle Dienste die übernommene Identität nutzen.",
      );
    } catch (e) {
      setPendingImport(null);
      setIdMsg(String(e));
    }
  }

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

      {/* Master-Identität umziehen (ADR 0006): Identität auf einen neuen
          Turnier-PC übertragen, ohne alle Geräte neu koppeln zu müssen. */}
      <section className="flex flex-col gap-3 rounded-xl border border-slate-200 bg-white p-5 shadow-sm">
        <h2 className="text-sm font-semibold text-slate-700">
          Master-Identität umziehen
        </h2>
        <p className="text-xs text-slate-500">
          Beim Wechsel auf einen neuen Turnier-PC die Identität mitnehmen — dann
          bleiben alle gekoppelten Geräte (Tablets, Monitore, ferne Hallen) ohne
          Neu-Koppeln verbunden. Am alten PC exportieren, am neuen importieren.
          Die Datei enthält den Kopplungs-Token — wie ein Passwort behandeln,
          und es darf immer nur <b>ein</b> Master gleichzeitig laufen.
        </p>
        <div className="flex flex-wrap gap-2.5">
          <ActionButton
            icon={Download}
            label="Identität exportieren"
            onClick={() => void handleExportIdentity()}
            title="Identitäts-Bündel als Datei speichern (ohne Passwörter)"
          />
          <ActionButton
            icon={Upload}
            label="Identität importieren"
            onClick={() => fileInput.current?.click()}
            title="Ein Identitäts-Bündel von einem anderen PC übernehmen"
          />
          <input
            ref={fileInput}
            type="file"
            accept="application/json,.json"
            className="hidden"
            onChange={(e) => {
              void handleImportFileChosen(e.target.files?.[0]);
              e.target.value = ""; // gleiche Datei erneut wählbar
            }}
          />
        </div>
        {pendingImport != null && (
          <div className="flex flex-col gap-2 rounded-lg border border-amber-300 bg-amber-50 p-3">
            <p className="text-xs font-medium text-amber-900">
              Identität wirklich übernehmen? Das überschreibt die Identität
              dieses PCs. Der bisherige Master darf danach nicht mehr laufen.
            </p>
            <div className="flex gap-2">
              <button
                onClick={() => void confirmImport()}
                className="rounded-lg bg-amber-600 px-3.5 py-2 text-sm font-medium text-white
                           transition-colors hover:bg-amber-700"
              >
                Ja, Identität übernehmen
              </button>
              <button
                onClick={() => setPendingImport(null)}
                className="rounded-lg bg-slate-100 px-3.5 py-2 text-sm font-medium text-slate-700
                           transition-colors hover:bg-slate-200"
              >
                Abbrechen
              </button>
            </div>
          </div>
        )}
        {idMsg !== "" && <p className="text-xs text-slate-500">{idMsg}</p>}
      </section>
    </main>
  );
}
