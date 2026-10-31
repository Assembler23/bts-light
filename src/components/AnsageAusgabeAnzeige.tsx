import { Speaker } from "lucide-react";
import { useEffect, useState } from "react";
import {
  ansageAusgabe,
  ansageAusgabeSetzen,
  ansageGeraete,
  type AusgabeInfo,
  type GeraetInfo,
} from "../api";
import type { AppConfig } from "../types";

// Windows-Endpunkt-IDs sind GUID-Texte, deren Groß-/Kleinschreibung nicht
// garantiert gleich bleibt — der Kern vergleicht ebenso.
const gleicheId = (a: string, b: string) =>
  a.trim().toLowerCase() === b.trim().toLowerCase();

/** Abfragetakt, solange die Seite sichtbar ist (Spec ansage-ausgabegeraet). */
const TAKT_MS = 3000;

/**
 * Zeigt auf der Seite Ansagen, worüber die Ansage läuft, und lässt das
 * Gerät wählen (PR 2). Ohne Wahl ist das der Windows-Standard; eine
 * App-Zuordnung im Lautstärkemixer kann die App nicht zuverlässig lesen —
 * deshalb der Hinweis darunter (D6). Die Wahl gilt nur für diesen PC.
 */
export function AnsageAusgabeAnzeige({
  config,
  onSaved,
}: {
  config: AppConfig;
  onSaved: (config: AppConfig) => void;
}) {
  const legacy = config.announce.legacy_output === true;
  const gewaehltId = config.announce.output_device ?? "";
  const gewaehltName = config.announce.output_device_name ?? "";
  const [info, setInfo] = useState<AusgabeInfo | null>(null);
  const [geraete, setGeraete] = useState<GeraetInfo[]>([]);
  const [fehler, setFehler] = useState("");

  useEffect(() => {
    let aktiv = true;
    // Höchstens eine Abfrage zugleich: Hängt sie (Treiber), stapeln sich
    // sonst im Kern alle 3 s neue Threads.
    let laeuft = false;
    const holen = () => {
      if (laeuft || document.visibilityState !== "visible") return;
      laeuft = true;
      Promise.all([
        ansageAusgabe().catch(() => null),
        ansageGeraete().catch(() => null),
      ])
        .then(([i, g]) => {
          if (!aktiv) return;
          setInfo(i);
          // Ein vorübergehender Fehler soll die Liste nicht leeren — sonst
          // spränge das Feld kurz auf „(nicht verfügbar)".
          if (g) setGeraete(g);
        })
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

  const waehlen = (id: string) => {
    const name = geraete.find((g) => gleicheId(g.id, id))?.name ?? "";
    setFehler("");
    ansageAusgabeSetzen(id, name)
      .then(onSaved)
      .catch((e) => setFehler(String(e)));
  };

  let text: string;
  if (legacy) {
    text = "Ausgabe: über das Programmfenster (Alter Abspielweg)";
  } else if (!info || !info.unterstuetzt) {
    text = "Ausgabe: über das Programmfenster";
  } else if (info.gewaehlt !== null && info.verfuegbar) {
    text = `Ausgabe: ${info.gewaehlt || "gewähltes Gerät"}`;
  } else if (info.gewaehlt !== null) {
    text = `${info.gewaehlt || "Gewähltes Gerät"} nicht verfügbar – Ansage läuft über den Windows-Standard (${info.standard ?? "kein Gerät gefunden"})`;
  } else {
    text = `Ausgabe: Windows-Standard (${info.standard ?? "kein Gerät gefunden"})`;
  }

  // Das gewählte Gerät bleibt in der Liste, auch wenn es gerade fehlt —
  // sonst sprünge das Feld still auf „Windows-Standard".
  // Den Listenwert der gewählten ID nehmen (Schreibweise wie von Windows),
  // damit das Feld sie trifft.
  const treffer = geraete.find((g) => gleicheId(g.id, gewaehltId));
  const feldWert = treffer ? treffer.id : gewaehltId;
  const fehlt = gewaehltId !== "" && !treffer;
  const waehlbar = !legacy && info?.unterstuetzt === true;

  return (
    <div className="flex flex-col gap-1 text-sm text-slate-600">
      <span className="inline-flex items-center gap-2">
        <Speaker size={16} className="shrink-0 text-slate-400" />
        {text}
      </span>
      {waehlbar && (
        <label className="flex items-center gap-2">
          <span className="shrink-0">Ansage über:</span>
          <select
            value={feldWert}
            onChange={(e) => waehlen(e.currentTarget.value)}
            className="min-w-0 flex-1 rounded-lg border border-slate-300 bg-white px-2 py-1 text-sm
                       focus:border-slate-500 focus:outline-none"
          >
            <option value="">Windows-Standard</option>
            {geraete.map((g) => (
              <option key={g.id} value={g.id}>
                {g.name}
              </option>
            ))}
            {fehlt && (
              <option value={gewaehltId}>
                {gewaehltName || "Gewähltes Gerät"} (nicht verfügbar)
              </option>
            )}
          </select>
        </label>
      )}
      {fehler && <span className="text-xs text-red-600">{fehler}</span>}
      <span className="text-xs text-slate-400">
        Kommt die Ansage aus dem falschen Lautsprecher? Gerät oben wählen —
        oder in Windows unter Lautstärkemixer bei „BTS Light“ das
        Ausgabegerät auf „Standard“ stellen. Die Wahl gilt nur für diesen PC.
      </span>
    </div>
  );
}
