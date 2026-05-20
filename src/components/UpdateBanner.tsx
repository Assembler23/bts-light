import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

type Phase =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "current"
  | "error";

interface UpdateState {
  phase: Phase;
  update: Update | null;
  /** Manuell auf ein Update prüfen (z. B. per Dashboard-Button). */
  checkNow: () => Promise<void>;
  /** Update herunterladen, installieren und die App neu starten. */
  install: () => Promise<void>;
}

const UpdateContext = createContext<UpdateState | null>(null);

export function useUpdate(): UpdateState {
  const ctx = useContext(UpdateContext);
  if (!ctx) {
    throw new Error("useUpdate muss innerhalb von <UpdateProvider> stehen");
  }
  return ctx;
}

/**
 * Hält den Update-Status app-weit: ein automatischer Check beim Start, dazu
 * ein manueller Check fürs Dashboard. Netzwerkfehler sind kein harter
 * Fehler – eine Turnierhalle ist oft offline.
 */
export function UpdateProvider({ children }: { children: ReactNode }) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [update, setUpdate] = useState<Update | null>(null);

  const checkNow = useCallback(async () => {
    setPhase("checking");
    try {
      const found = await check();
      if (found) {
        setUpdate(found);
        setPhase("available");
      } else {
        setUpdate(null);
        setPhase("current");
      }
    } catch {
      setPhase("error");
    }
  }, []);

  const install = useCallback(async () => {
    if (!update) return;
    setPhase("downloading");
    try {
      await update.downloadAndInstall();
      await relaunch();
    } catch {
      setPhase("error");
    }
  }, [update]);

  // Automatischer Check beim App-Start.
  useEffect(() => {
    void checkNow();
  }, [checkNow]);

  return (
    <UpdateContext.Provider value={{ phase, update, checkNow, install }}>
      {children}
    </UpdateContext.Provider>
  );
}

/** Nicht-blockierendes Banner oben, sobald ein Update bereitsteht. */
export function UpdateBanner() {
  const { phase, update, install } = useUpdate();
  if (phase !== "available" && phase !== "downloading") return null;

  return (
    <div className="flex items-center justify-between gap-3 bg-amber-100 px-4 py-2 text-sm text-amber-900">
      <span>
        Update verfügbar{update ? ` (v${update.version})` : ""} – neue
        Funktionen &amp; Fehlerbehebungen.
      </span>
      <button
        onClick={() => void install()}
        disabled={phase === "downloading"}
        className="shrink-0 rounded-lg bg-amber-500 px-3 py-1 font-medium text-white
                   disabled:opacity-60"
      >
        {phase === "downloading"
          ? "Wird installiert …"
          : "Herunterladen & neu starten"}
      </button>
    </div>
  );
}
