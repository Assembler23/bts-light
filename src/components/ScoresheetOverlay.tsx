import { useEffect, useState } from "react";
import { X } from "lucide-react";
import { matchScoresheetHtml } from "../api";

/**
 * Anzeige- und Druckfenster für Schiedsrichterzettel.
 *
 * Das fertige Dokument kommt als HTML **vom Kern** (Tauri-Command
 * `match_scoresheet_html`, S-R1: kein `fetch` aus React auf
 * `127.0.0.1:8088`) und wird in einem `iframe srcdoc` gezeigt. Gedruckt
 * wird über den WebView, „als PDF speichern" über den Systemdialog des
 * Druckers — deshalb braucht es weder eine PDF-Abhängigkeit noch
 * `dialog:allow-save` (ADR 0039).
 *
 * `vorab` schaltet auf den **Vorabzettel**: das leere Blatt eines noch
 * ausstehenden Spiels (Spec `schiedsrichterzettel-autodruck`). Nur dann
 * liefert der Kern auch ohne Aufzeichnung ein Dokument.
 *
 * Wurde zu einem Spiel **nichts gezählt**, bietet die Meldung einen zweiten
 * Knopf an: „Leeres Blatt" holt denselben Vorabzettel nach. Bewusst als
 * eigener Klick statt als stiller Rückfall — sonst sähe der Nachdruck eines
 * per Papier erfassten Spiels aus wie ein gezählter Zettel.
 */
export function ScoresheetOverlay({
  matchIds,
  titel,
  vorab = false,
  onClose,
}: {
  matchIds: number[];
  titel: string;
  vorab?: boolean;
  onClose: () => void;
}) {
  const [html, setHtml] = useState<string | null | "fehlt">(null);
  /** Wird gerade das leere Blatt gezeigt? Startet auf `vorab` und kann über
   *  den Knopf in der Meldung nachträglich eingeschaltet werden.
   *
   *  Zurückgesetzt wird er **nicht** hier, sondern durch Neuaufbau: Die
   *  Aufrufer geben dem Bauteil die Spiele als `key`, ein Wechsel erzeugt
   *  also eine frische Instanz. Ein eigener Rücksetz-Effekt liefe im selben
   *  Durchlauf wie der Abruf unten und stieße erst einen Abruf in der alten
   *  Betriebsart an. */
  const [leer, setLeer] = useState(vorab);

  useEffect(() => {
    let alive = true;
    setHtml(null);
    matchScoresheetHtml(matchIds, leer)
      .then((doc) => {
        if (alive) setHtml(doc ?? "fehlt");
      })
      .catch(() => {
        if (alive) setHtml("fehlt");
      });
    return () => {
      alive = false;
    };
    // Die Kennungen als Zeichenkette: Ein neues Array mit gleichem Inhalt
    // soll den Abruf nicht wiederholen.
  }, [matchIds.join(","), leer]);

  useEffect(() => {
    const zu = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", zu);
    return () => window.removeEventListener("keydown", zu);
  }, [onClose]);

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="zettel-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/60 p-4"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      {/* Breit genug für ein A4-Blatt quer (297 mm ≈ 1123 px) samt Rahmen,
          aber nie breiter als der Bildschirm. */}
      <div
        className="flex h-[96vh] w-[98vw] max-w-[1400px] flex-col overflow-hidden
                   rounded-xl bg-white shadow-xl"
      >
        <div className="flex items-center justify-between border-b border-slate-200 px-5 py-3">
          <h2 id="zettel-title" className="font-semibold text-slate-800">
            Schiedsrichterzettel{leer ? " (leer)" : ""} — {titel}
          </h2>
          <div className="flex items-center gap-2">
            <button
              onClick={() => {
                const rahmen = document.getElementById(
                  "zettel-frame",
                ) as HTMLIFrameElement | null;
                rahmen?.contentWindow?.focus();
                rahmen?.contentWindow?.print();
              }}
              disabled={typeof html !== "string" || html === "fehlt"}
              className="rounded bg-slate-800 px-3 py-1.5 text-sm font-medium text-white
                         hover:bg-slate-700 disabled:opacity-40"
            >
              Drucken
            </button>
            <button
              onClick={onClose}
              aria-label="Zettel schließen"
              className="rounded p-1 text-slate-400 hover:bg-slate-100"
            >
              <X size={18} />
            </button>
          </div>
        </div>
        <div className="flex-1 overflow-hidden bg-slate-100 p-2">
          {html === null && (
            <p className="p-4 text-sm text-slate-500">Zettel wird erzeugt …</p>
          )}
          {html === "fehlt" && (
            <div className="p-4 text-sm text-slate-500">
              <p>
                {leer
                  ? "Für dieses Spiel lässt sich kein Blatt erzeugen — steht es noch im aktuellen Turnierstand?"
                  : "Zu diesem Spiel wurde nichts gezählt — deshalb gibt es keinen ausgefüllten Zettel."}
              </p>
              {!leer && (
                <button
                  onClick={() => setLeer(true)}
                  className="mt-3 rounded bg-slate-800 px-3 py-1.5 text-sm font-medium
                             text-white hover:bg-slate-700"
                >
                  Leeres Blatt
                </button>
              )}
            </div>
          )}
          {typeof html === "string" && html !== "fehlt" && (
            <iframe
              id="zettel-frame"
              title="Schiedsrichterzettel"
              srcDoc={html}
              /* Das Dokument enthält bewusst kein Skript (ADR 0039); die
                 Sandbox macht das durchsetzbar statt zugesagt.
                 `allow-modals` bleibt nötig, damit `print()` den
                 Druckdialog öffnen darf. */
              sandbox="allow-same-origin allow-modals"
              className="h-full w-full rounded border border-slate-300 bg-white"
            />
          )}
        </div>
      </div>
    </div>
  );
}
