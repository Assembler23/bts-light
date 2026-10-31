import { Speaker } from "lucide-react";
import { useEffect, useState } from "react";
import { ansageAusgabe, type AusgabeInfo } from "../api";

/** Abfragetakt, solange die Seite sichtbar ist (Spec ansage-ausgabegeraet). */
const TAKT_MS = 3000;

/**
 * Zeigt auf der Seite Ansagen, worüber die Ansage läuft. Ohne gezielte Wahl
 * ist das der Windows-Standard; eine App-Zuordnung im Lautstärkemixer kann
 * die App nicht zuverlässig lesen — deshalb der Hinweis darunter (D6).
 */
export function AnsageAusgabeAnzeige({ legacy }: { legacy: boolean }) {
  const [info, setInfo] = useState<AusgabeInfo | null>(null);

  useEffect(() => {
    let aktiv = true;
    // Höchstens eine Abfrage zugleich: Hängt sie (Treiber), stapeln sich
    // sonst im Kern alle 3 s neue Threads.
    let laeuft = false;
    const holen = () => {
      if (laeuft || document.visibilityState !== "visible") return;
      laeuft = true;
      ansageAusgabe()
        .then((i) => aktiv && setInfo(i))
        .catch(() => aktiv && setInfo(null))
        .finally(() => {
          laeuft = false;
        });
    };
    holen();
    const t = window.setInterval(holen, TAKT_MS);
    return () => {
      aktiv = false;
      window.clearInterval(t);
    };
  }, []);

  let text: string;
  if (legacy) {
    text = "Ausgabe: über das Programmfenster (Alter Abspielweg)";
  } else if (!info || !info.unterstuetzt) {
    text = "Ausgabe: über das Programmfenster";
  } else {
    text = `Ausgabe: Windows-Standard (${info.standard ?? "kein Gerät gefunden"})`;
  }

  return (
    <div className="flex flex-col gap-1 text-sm text-slate-600">
      <span className="inline-flex items-center gap-2">
        <Speaker size={16} className="shrink-0 text-slate-400" />
        {text}
      </span>
      <span className="text-xs text-slate-400">
        Kommt die Ansage aus dem falschen Lautsprecher? In Windows unter
        Lautstärkemixer bei „BTS Light“ das Ausgabegerät auf „Standard“
        stellen.
      </span>
    </div>
  );
}
