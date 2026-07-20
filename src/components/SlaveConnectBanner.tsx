import { useEffect, useRef, useState } from "react";
import { Link2, Unlink, X } from "lucide-react";
import type { SlaveInfo } from "../types";

/** Ein Zustandswechsel einer fernen Halle, aus prev/now abgeleitet. Rein, damit
 *  die Entscheidungslogik ohne React nachvollziehbar (und leicht prüfbar) ist.
 *  `prev`: zuletzt bekannter Online-Status (undefined = erstmals gesehen). */
export function slaveTransition(
  prev: boolean | undefined,
  now: boolean,
): "connected" | "reconnected" | "offline" | null {
  if (now && prev !== true)
    return prev === undefined ? "connected" : "reconnected";
  if (!now && prev === true) return "offline";
  return null;
}

/**
 * Master-Hinweis bei Mehr-Hallen-Turnieren (ADR 0006, Geräte-Übersicht):
 * - Verbindet sich eine ferne Halle NEU oder kommt nach einem Ausfall zurück,
 *   erscheint kurz ein grüner Banner (Kopplung geklappt).
 * - Bricht eine zuvor verbundene ferne Halle WEG (z. B. nach einem PC-Wechsel
 *   oder Netzausfall), erscheint ein **persistenter amberfarbener Warn-Banner**,
 *   damit das Wegbrechen sichtbar wird statt still zu passieren.
 * Bereits beim App-Start verbundene Hallen lösen keinen Banner aus (Baseline);
 * ihren Dauerstatus zeigt die Kopfzeile.
 */
export function SlaveConnectBanner({ slaves }: { slaves: SlaveInfo[] }) {
  // id → zuletzt gesehener Online-Status; null = Baseline steht noch aus.
  const known = useRef<Map<string, boolean> | null>(null);
  const hideTimer = useRef<number | null>(null);
  const [notice, setNotice] = useState<{
    text: string;
    tone: "ok" | "warn";
  } | null>(null);

  useEffect(() => {
    if (known.current === null) {
      known.current = new Map(slaves.map((s) => [s.id, s.online]));
      return;
    }
    const map = known.current;
    // Ereignisse des Ticks sammeln, dann entscheiden: eine Offline-Warnung
    // GEWINNT gegen eine gleichzeitige (Wieder-)Verbindung — sonst könnte ein
    // grüner Hinweis die persistente Warnung im selben Poll überschreiben,
    // wenn zwei Hallen zugleich wechseln (Review-Befund).
    let okNotice: string | null = null;
    let offlineNotice: string | null = null;
    for (const s of slaves) {
      const prev = map.get(s.id);
      map.set(s.id, s.online);
      const event = slaveTransition(prev, s.online);
      if (event === null) continue;
      const hall = s.hall || "Ferne Halle";
      if (event === "offline") {
        offlineNotice = `Ferne Halle „${hall}" ist offline gegangen`;
      } else {
        okNotice =
          event === "connected"
            ? `Ferne Halle „${hall}" hat sich verbunden ✓`
            : `Ferne Halle „${hall}" ist wieder verbunden ✓`;
      }
    }
    if (offlineNotice !== null) {
      // Persistente Warnung (kein Auto-Ausblenden).
      if (hideTimer.current !== null) {
        window.clearTimeout(hideTimer.current);
        hideTimer.current = null;
      }
      setNotice({ text: offlineNotice, tone: "warn" });
    } else if (okNotice !== null) {
      // (Wieder-)Verbindung: grüner Hinweis, blendet nach 12 s aus.
      setNotice({ text: okNotice, tone: "ok" });
      if (hideTimer.current !== null) window.clearTimeout(hideTimer.current);
      hideTimer.current = window.setTimeout(() => {
        setNotice(null);
        hideTimer.current = null;
      }, 12000);
    }
  }, [slaves]);

  useEffect(
    () => () => {
      if (hideTimer.current !== null) window.clearTimeout(hideTimer.current);
    },
    [],
  );

  if (!notice) return null;
  const warn = notice.tone === "warn";
  return (
    <div
      className={`flex items-center gap-2 px-4 py-2 text-sm font-medium text-white ${
        warn ? "bg-amber-600" : "bg-emerald-600"
      }`}
    >
      {warn ? (
        <Unlink size={16} className="shrink-0" />
      ) : (
        <Link2 size={16} className="shrink-0" />
      )}
      <span className="min-w-0">{notice.text}</span>
      <button
        type="button"
        title="Hinweis ausblenden"
        onClick={() => setNotice(null)}
        className={`ml-auto shrink-0 rounded p-0.5 transition-colors ${
          warn ? "hover:bg-amber-700" : "hover:bg-emerald-700"
        }`}
      >
        <X size={16} />
      </button>
    </div>
  );
}
