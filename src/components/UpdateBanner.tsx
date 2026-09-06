import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { Download } from "lucide-react";
import {
  updateCheck,
  updateInfo,
  updateInstallNow,
  updateSetInstallOnExit,
} from "../api";
import type { UpdateInfo } from "../types";

interface UpdateState {
  /** Phase des Ablaufs (siehe `UpdateInfo.phase`). */
  phase: UpdateInfo["phase"];
  info: UpdateInfo | null;
  /** Manuell auf ein Update prüfen (Wartungsseite). */
  checkNow: () => Promise<void>;
  /** Geladenes Update jetzt einbauen (App startet neu). */
  installNow: () => Promise<void>;
  /** Einbau beim Beenden vormerken oder lösen. */
  setInstallOnExit: (an: boolean) => Promise<void>;
  /** Fehlertext des letzten Einbau-Versuchs (leer = keiner). */
  installError: string;
}

const UpdateContext = createContext<UpdateState | null>(null);

export function useUpdate(): UpdateState {
  const ctx = useContext(UpdateContext);
  if (!ctx) {
    throw new Error("useUpdate muss innerhalb von <UpdateProvider> stehen");
  }
  return ctx;
}

/** Phasen, in denen sich etwas tut — dann liest das Banner im kurzen Takt. */
const AKTIV: ReadonlySet<UpdateInfo["phase"]> = new Set([
  "checking",
  "downloading",
  "installing",
]);

/** Lese-Takt je Phase: schnell, solange etwas läuft; „bereit" kann den
 *  ganzen Turniertag stehen und baut je Abfrage die Feldübersicht — dafür
 *  reichen 10 s; sonst 30 s (fängt einen Check von der Wartungsseite ein). */
export function leseTaktMs(phase: UpdateInfo["phase"]): number {
  if (AKTIV.has(phase)) return 2000;
  if (phase === "ready") return 10_000;
  return 30_000;
}

/**
 * Hält den Update-Status app-weit (Spec `update-im-turnierbetrieb`). Der
 * Ablauf selbst lebt im Rust-Kern: einmal beim Start anstoßen, danach den
 * Stand lesen — im 2-s-Takt, solange Laden oder Einbau anstehen, sonst
 * gemächlich. Netzwerkfehler sind kein harter Fehler: eine Turnierhalle ist
 * oft offline.
 */
export function UpdateProvider({ children }: { children: ReactNode }) {
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [installError, setInstallError] = useState("");
  const phase = info?.phase ?? "idle";

  const lesen = useCallback(async () => {
    try {
      setInfo(await updateInfo());
    } catch {
      /* Rust-Seite nicht erreichbar: Stand bleibt stehen. */
    }
  }, []);

  const checkNow = useCallback(async () => {
    try {
      await updateCheck();
    } catch {
      /* wird über die Phase `error` sichtbar */
    }
    await lesen();
  }, [lesen]);

  const installNow = useCallback(async () => {
    setInstallError("");
    try {
      await updateInstallNow();
    } catch (e) {
      // Der Installer ließ sich nicht starten — die Turnierleitung muss das
      // sehen, sonst wartet sie auf einen Neustart, der nie kommt.
      setInstallError(String(e));
    }
    await lesen();
  }, [lesen]);

  const setInstallOnExit = useCallback(
    async (an: boolean) => {
      try {
        await updateSetInstallOnExit(an);
      } catch {
        /* kein Paket geladen — Banner zeigt den Stand ohnehin */
      }
      await lesen();
    },
    [lesen],
  );

  // Automatischer Check beim App-Start.
  useEffect(() => {
    void checkNow();
  }, [checkNow]);

  useEffect(() => {
    const id = setInterval(() => void lesen(), leseTaktMs(phase));
    return () => clearInterval(id);
  }, [phase, lesen]);

  return (
    <UpdateContext.Provider
      value={{
        phase,
        info,
        checkNow,
        installNow,
        setInstallOnExit,
        installError,
      }}
    >
      {children}
    </UpdateContext.Provider>
  );
}

/** Hinweis auf die Unterbrechung, abhängig von der Lage in der Halle. */
export function unterbrechungsHinweis(info: UpdateInfo): string {
  if (!info.sync_running) return "Die App startet dafür kurz neu.";
  if (info.occupied_courts === 0) {
    return "Kein Spiel läuft – die Übertragung setzt nach dem Neustart von selbst wieder ein.";
  }
  const felder =
    info.occupied_courts === 1
      ? "Auf 1 Feld läuft ein Spiel"
      : `Auf ${info.occupied_courts} Feldern laufen Spiele`;
  return `${felder} – der Neustart unterbricht etwa 20 s, die Tablets zählen weiter und die Übertragung setzt von selbst wieder ein.`;
}

/** Nicht-blockierendes Banner oben, sobald ein Update geladen wird oder bereitliegt. */
export function UpdateBanner() {
  const { phase, info, installNow, setInstallOnExit, installError } =
    useUpdate();
  if (!info) return null;
  // Ein gescheiterter Einbau lässt das Paket bereit (Phase `ready`) — der
  // Fehlertext kommt aus der abgelehnten Antwort und hat Vorrang.
  if (installError) {
    return (
      <div className="flex items-center gap-2 bg-red-100 px-4 py-2 text-sm text-red-900">
        <Download size={16} className="shrink-0" />
        {installError}
      </div>
    );
  }
  if (phase !== "downloading" && phase !== "ready" && phase !== "installing") {
    return null;
  }
  const version = info.version ? ` (v${info.version})` : "";

  if (phase === "downloading") {
    return (
      <div className="flex items-center gap-2 bg-amber-100 px-4 py-2 text-sm text-amber-900">
        <Download size={16} className="shrink-0" />
        Update{version} wird im Hintergrund geladen …
      </div>
    );
  }
  if (phase === "installing") {
    return (
      <div className="flex items-center gap-2 bg-amber-100 px-4 py-2 text-sm text-amber-900">
        <Download size={16} className="shrink-0" />
        Update{version} wird eingebaut – die App startet gleich neu.
      </div>
    );
  }

  return (
    <div className="flex flex-wrap items-center justify-between gap-3 bg-amber-100 px-4 py-2 text-sm text-amber-900">
      <span className="flex items-start gap-2">
        <Download size={16} className="mt-0.5 shrink-0" />
        <span>
          <span className="font-medium">Update{version} liegt bereit.</span>{" "}
          {info.install_on_exit
            ? "Es wird beim Beenden von bts-light still eingebaut."
            : unterbrechungsHinweis(info)}
        </span>
      </span>
      <span className="flex shrink-0 items-center gap-2">
        {info.install_on_exit ? (
          <button
            onClick={() => void setInstallOnExit(false)}
            className="rounded-lg border border-amber-500 px-3 py-1 font-medium text-amber-900
                       transition-colors hover:bg-amber-200"
          >
            Doch nicht beim Beenden
          </button>
        ) : (
          <button
            onClick={() => void setInstallOnExit(true)}
            className="rounded-lg border border-amber-500 px-3 py-1 font-medium text-amber-900
                       transition-colors hover:bg-amber-200"
          >
            Beim Beenden einbauen
          </button>
        )}
        <button
          onClick={() => void installNow()}
          className="rounded-lg bg-amber-500 px-3 py-1 font-medium text-white
                     transition-colors hover:bg-amber-600"
        >
          Jetzt neu starten
        </button>
      </span>
    </div>
  );
}
