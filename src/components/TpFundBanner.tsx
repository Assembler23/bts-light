import { useCallback, useEffect, useRef, useState } from "react";
import { Database, X } from "lucide-react";
import {
  loadConfig,
  tpFindenAntwort,
  tpFindenHinweisWeg,
  tpFindenPruefen,
  tpFindenStatus,
} from "../api";
import { tpFrageText } from "../io/tpWahl.mjs";
import { shortTournamentGuid } from "../tournamentGuid";
import type { AppConfig, TpFindenStatus, TpFrageSicht } from "../types";

interface Props {
  /** Die Config ist geladen — vorher wird weder gesucht noch gefragt. */
  bereit: boolean;
  /** Schreibzähler, den die Oberfläche kennt (`config.tp_upload.stand`). */
  stand: number;
  /** Die Automatik hat außerhalb des Setups geschrieben: neuen Stand übernehmen. */
  onConfig: (config: AppConfig) => void;
  /** Übertragung stoppen und starten (nach bestätigtem Kennungs-Wechsel). */
  onNeustart: () => Promise<void>;
}

/**
 * App-weite Rückfrage der TP-Datei-Automatik (Spec `tp-datei-finden`, PR 2,
 * ADR 0097): In BTP ist eine Datei geöffnet, die nicht erkennbar zum
 * eingetragenen Turnier gehört.
 *
 * **Warum app-weit?** Die Automatik sucht beim App-Start und im Takt der
 * Übertragung, egal welcher Tab offen ist. Sie übernimmt eine fremde Kennung
 * nie still — die Kennung steuert Liveticker, Check-In und Aushang.
 *
 * Nebenbei hält die Komponente die Config der Oberfläche frisch: Zieht die
 * Automatik einen Pfad still nach, steigt der Schreibzähler, und die
 * Oberfläche lädt die Config neu (sonst schriebe das nächste Speichern den
 * alten Stand zurück — ADR 0098).
 */
export function TpFundBanner({ bereit, stand, onConfig, onNeustart }: Props) {
  const [frage, setFrage] = useState<TpFrageSicht | null>(null);
  /** Dateiname einer still übernommenen Datei (gleiche Kennung, anderer Ort). */
  const [gewechselt, setGewechselt] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [fehler, setFehler] = useState<string | null>(null);
  // Nur die jüngste Abfrage zählt: Eine Antwort, die vor dem Klick unterwegs
  // war, darf die beantwortete Frage nicht kurz zurückholen.
  const abfrage = useRef(0);
  const standRef = useRef(stand);
  standRef.current = stand;
  const onConfigRef = useRef(onConfig);
  onConfigRef.current = onConfig;

  const uebernehmen = useCallback((nr: number, s: TpFindenStatus) => {
    if (nr !== abfrage.current) return;
    setFrage(s.frage);
    setGewechselt(s.gewechselt);
    // Eine neue Frage löst die Meldung zur alten ab.
    if (s.frage) setFehler(null);
    if (s.stand !== standRef.current) {
      loadConfig()
        .then((c) => onConfigRef.current(c))
        .catch(() => {});
    }
  }, []);

  useEffect(() => {
    if (!bereit) return;
    // Beim App-Start einmal wirklich suchen, danach nur den Stand abholen —
    // gesucht wird dann im Takt der Übertragung.
    const start = ++abfrage.current;
    tpFindenPruefen()
      .then((s) => uebernehmen(start, s))
      .catch(() => {});
    const id = window.setInterval(() => {
      const nr = ++abfrage.current;
      tpFindenStatus()
        .then((s) => uebernehmen(nr, s))
        .catch(() => {});
    }, 10_000);
    return () => window.clearInterval(id);
  }, [bereit, uebernehmen]);

  if (!frage) {
    // Eine abgelehnte Antwort („Datei hat sich geändert") muss sichtbar sein.
    if (!fehler && gewechselt) {
      // Still übernommen heißt nicht unsichtbar: Wer in BTP versehentlich
      // eine Sicherungskopie geöffnet hat, soll es hier merken.
      return (
        <div className="flex items-center gap-2 bg-sky-600 px-4 py-2 text-sm font-medium text-white">
          <Database size={16} className="shrink-0" />
          <span className="min-w-0">
            TP-Datei gewechselt: In BTP ist jetzt „{gewechselt}“ geöffnet — bts-light
            verwendet diese Datei.
          </span>
          <button
            type="button"
            title="Hinweis ausblenden"
            onClick={() => {
              setGewechselt(null);
              abfrage.current++;
              tpFindenHinweisWeg().catch(() => {});
            }}
            className="ml-auto shrink-0 rounded p-0.5 transition-colors hover:bg-sky-700"
          >
            <X size={16} />
          </button>
        </div>
      );
    }
    if (!fehler) return null;
    return (
      <div className="flex items-center gap-2 bg-rose-600 px-4 py-2 text-sm font-medium text-white">
        <Database size={16} className="shrink-0" />
        <span className="min-w-0">TP-Datei: {fehler}</span>
        <button
          type="button"
          title="Hinweis ausblenden"
          onClick={() => setFehler(null)}
          className="ml-auto shrink-0 rounded p-0.5 transition-colors hover:bg-rose-700"
        >
          <X size={16} />
        </button>
      </div>
    );
  }

  const antworten = (ja: boolean) => {
    setBusy(true);
    setFehler(null);
    abfrage.current++;
    tpFindenAntwort(ja)
      .then(async (a) => {
        setFrage(null);
        onConfigRef.current(a.config);
        if (a.neustart) await onNeustart();
      })
      .catch((e) => {
        // Abgelehnt (Datei geändert/nicht lesbar): Die Frage ist im Kern
        // verworfen; der nächste Takt stellt sie bei Bedarf neu.
        setFrage(null);
        setFehler(String(e));
      })
      .finally(() => setBusy(false));
  };

  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-2 bg-amber-500 px-4 py-2 text-sm font-medium text-white">
      <Database size={16} className="shrink-0" />
      <span className="min-w-0 flex-1 basis-80">
        {tpFrageText(frage, shortTournamentGuid)}
        <span className="ml-1 font-normal opacity-90">
          Bei „Nein“ wird die automatische Suche ausgeschaltet.
        </span>
      </span>
      <button
        type="button"
        disabled={busy}
        onClick={() => antworten(true)}
        className="shrink-0 rounded bg-white px-3 py-1 text-amber-700 transition-colors
                   hover:bg-amber-50 disabled:opacity-60"
      >
        {frage.datei_guid === null ? "Ja, Datei verwenden" : "Ja, übernehmen"}
      </button>
      <button
        type="button"
        disabled={busy}
        onClick={() => antworten(false)}
        className="shrink-0 rounded px-3 py-1 transition-colors hover:bg-amber-600
                   disabled:opacity-60"
      >
        Nein
      </button>
    </div>
  );
}
