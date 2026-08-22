import { useEffect, useState } from "react";
import { X } from "lucide-react";
import { aushangHtml } from "../api";

/**
 * Anzeige- und Druckfenster für den Hallen-Aushang: ein A4-Blatt mit den
 * QR-Codes zur Teilnehmerliste und zum Liveticker (`docs/aushang.md`).
 *
 * Wie beim Schiedsrichterzettel kommt das fertige Dokument **vom Kern**
 * (Tauri-Command `aushang_html`, S-R1: kein `fetch` aus React auf
 * `127.0.0.1:8088`) und wird in einem `iframe srcdoc` gezeigt. Gedruckt wird
 * über den WebView, „als PDF speichern" über den Systemdialog des Druckers
 * (ADR 0039).
 *
 * Fehlt die öffentliche Live-Seite in den Einstellungen, liefert der Kern
 * statt eines Blattes einen sprechenden Grund — der steht dann hier, damit
 * die Turnierleitung weiß, was zu tun ist.
 */
export function AushangOverlay({ onClose }: { onClose: () => void }) {
  const [html, setHtml] = useState<string | null>(null);
  const [fehler, setFehler] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    aushangHtml()
      .then((doc) => {
        if (alive) setHtml(doc);
      })
      .catch((e) => {
        if (alive) setFehler(typeof e === "string" ? e : String(e));
      });
    return () => {
      alive = false;
    };
  }, []);

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
      aria-labelledby="aushang-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/60 p-4"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      {/* Breit genug für ein A4-Blatt hoch (210 mm ≈ 794 px) samt Rahmen,
          aber nie breiter als der Bildschirm. */}
      <div
        className="flex h-[96vh] w-[98vw] max-w-[900px] flex-col overflow-hidden
                   rounded-xl bg-white shadow-xl"
      >
        <div className="flex items-center justify-between border-b border-slate-200 px-5 py-3">
          <h2 id="aushang-title" className="font-semibold text-slate-800">
            Aushang für die Halle
          </h2>
          <div className="flex items-center gap-2">
            <button
              onClick={() => {
                const rahmen = document.getElementById(
                  "aushang-frame",
                ) as HTMLIFrameElement | null;
                rahmen?.contentWindow?.focus();
                rahmen?.contentWindow?.print();
              }}
              disabled={html === null}
              className="rounded bg-slate-800 px-3 py-1.5 text-sm font-medium text-white
                         hover:bg-slate-700 disabled:opacity-40"
            >
              Drucken
            </button>
            <button
              onClick={onClose}
              aria-label="Aushang schließen"
              className="rounded p-1 text-slate-400 hover:bg-slate-100"
            >
              <X size={18} />
            </button>
          </div>
        </div>
        <div className="flex-1 overflow-hidden bg-slate-100 p-2">
          {html === null && fehler === null && (
            <p className="p-4 text-sm text-slate-500">Aushang wird erzeugt …</p>
          )}
          {fehler !== null && (
            <p className="p-4 text-sm text-slate-600">{fehler}</p>
          )}
          {html !== null && (
            <iframe
              id="aushang-frame"
              title="Aushang"
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
