import { useEffect, useRef, useState } from "react";
import { Link2, X } from "lucide-react";
import type { SlaveInfo } from "../types";

/**
 * Master-Hinweis bei Mehr-Hallen-Turnieren: Wenn sich eine ferne Halle
 * (Cloud-Ansage-Slave) NEU verbindet oder nach einem Ausfall zurückkommt,
 * erscheint kurz ein grüner Banner — die Turnierleitung sieht sofort, dass
 * die Kopplung geklappt hat, ohne auf den kleinen Kopfzeilen-Punkt achten
 * zu müssen. Bereits beim App-Start verbundene Hallen werden nicht gemeldet
 * (Baseline), die zeigt die Kopfzeile dauerhaft.
 */
export function SlaveConnectBanner({ slaves }: { slaves: SlaveInfo[] }) {
  // id → zuletzt gesehener Online-Status; null = Baseline steht noch aus.
  const known = useRef<Map<string, boolean> | null>(null);
  const hideTimer = useRef<number | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    if (known.current === null) {
      known.current = new Map(slaves.map((s) => [s.id, s.online]));
      return;
    }
    const map = known.current;
    for (const s of slaves) {
      const prev = map.get(s.id);
      map.set(s.id, s.online);
      if (!s.online || prev === true) continue;
      const hall = s.hall || "Ferne Halle";
      setNotice(
        prev === undefined
          ? `Ferne Halle „${hall}" hat sich verbunden ✓`
          : `Ferne Halle „${hall}" ist wieder verbunden ✓`,
      );
      if (hideTimer.current !== null) window.clearTimeout(hideTimer.current);
      hideTimer.current = window.setTimeout(() => setNotice(null), 12000);
    }
  }, [slaves]);

  useEffect(
    () => () => {
      if (hideTimer.current !== null) window.clearTimeout(hideTimer.current);
    },
    [],
  );

  if (!notice) return null;
  return (
    <div className="flex items-center gap-2 bg-emerald-600 px-4 py-2 text-sm font-medium text-white">
      <Link2 size={16} className="shrink-0" />
      <span className="min-w-0">{notice}</span>
      <button
        type="button"
        title="Hinweis ausblenden"
        onClick={() => setNotice(null)}
        className="ml-auto shrink-0 rounded p-0.5 transition-colors hover:bg-emerald-700"
      >
        <X size={16} />
      </button>
    </div>
  );
}
