import { useEffect, useState } from "react";
import { HeartPulse, Megaphone, MonitorX } from "lucide-react";
import { tabletOverview } from "../api";
import type { CourtOverview } from "../types";

/**
 * App-weite Meldeleiste: zeigt auf JEDER Seite, wenn ein Tablet eine
 * Verletzung gemeldet oder die Turnierleitung ans Feld gerufen hat – mit
 * Feldnummer, damit die Turnierleitung sofort eingreifen kann. Pollt die
 * Felder-Übersicht alle 3 s.
 *
 * Dazu der Hinweis, wenn ein anderer PC dasselbe Turnier in der Cloud hält
 * (Spec relay-haerten, PR 9): Dann erreichen Cloud-Tablets und -Monitore
 * DIESEN PC nicht — ohne den Hinweis sähe man davon hier nichts.
 */
export function AlertBanner() {
  const [courts, setCourts] = useState<CourtOverview[]>([]);
  const [cloudHinweis, setCloudHinweis] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    const tick = () => {
      tabletOverview()
        .then((i) => {
          if (!active) return;
          setCourts(i.courts);
          setCloudHinweis(i.cloud_hinweis ?? null);
        })
        .catch(() => {});
    };
    tick();
    const id = setInterval(tick, 3000);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, []);

  const injuries = courts.filter((c) => c.injury).map((c) => c.court);
  const officials = courts.filter((c) => c.official_call).map((c) => c.court);
  if (injuries.length === 0 && officials.length === 0 && !cloudHinweis) return null;

  return (
    <div className="flex flex-col gap-1 bg-rose-600 px-4 py-2 text-sm font-medium text-white">
      {cloudHinweis && (
        <div className="flex items-center gap-2">
          <MonitorX size={16} className="shrink-0" />
          {cloudHinweis} – Cloud-Tablets und -Monitore erreichen diesen PC
          nicht. Bitte die Übertragung am anderen PC stoppen.
        </div>
      )}
      {officials.length > 0 && (
        <div className="flex items-center gap-2">
          <Megaphone size={16} className="shrink-0" />
          Turnierleitung ans Feld gerufen: {officials.join(", ")}
        </div>
      )}
      {injuries.length > 0 && (
        <div className="flex items-center gap-2">
          <HeartPulse size={16} className="shrink-0" />
          Verletzung / Behandlung läuft: {injuries.join(", ")}
        </div>
      )}
    </div>
  );
}
