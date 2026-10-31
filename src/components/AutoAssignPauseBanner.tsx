import { useCallback, useEffect, useRef, useState } from "react";
import { CirclePause } from "lucide-react";
import { autoAssignStatus, setAutoAssignPaused } from "../api";

/**
 * App-weiter Hinweis „Automatik pausiert" mit Knopf „Fortsetzen".
 *
 * **Warum?** Die automatische Feldvergabe wird im TL-Web pausiert, und die
 * Pause überlebt seit v0.9.323 Neustart und Update. Am PC sah man sie bis
 * v0.9.325 gar nicht — und ohne TL-Gerät ließ sie sich nur über den Umweg
 * „Automatik in den Einstellungen aus- und wieder einschalten" aufheben.
 *
 * Erscheint nur, wenn die Automatik eingeschaltet **und** pausiert ist.
 * Steht app-weit (wie der Druckhinweis): Die Vergabe ruht, egal welcher Tab
 * gerade offen ist.
 */
export function AutoAssignPauseBanner() {
  const [paused, setPaused] = useState(false);
  const [busy, setBusy] = useState(false);
  const [fehler, setFehler] = useState<string | null>(null);

  // Nur die jüngste Abfrage zählt: Eine Antwort, die schon vor dem Klick auf
  // „Fortsetzen" unterwegs war, darf den Balken nicht kurz zurückholen.
  const abfrage = useRef(0);

  const holen = useCallback(() => {
    const nr = ++abfrage.current;
    autoAssignStatus()
      .then((s) => {
        if (nr !== abfrage.current) return;
        setPaused(s.paused);
        // Ist die Pause weg (auch im TL-Web aufgehoben), gehört eine alte
        // Fehlermeldung nicht zur nächsten Pause.
        if (!s.paused) setFehler(null);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    holen();
    // Takt wie die übrigen Hinweise, etwas zügiger: Wer im TL-Web pausiert,
    // soll das am PC binnen Sekunden sehen.
    const id = window.setInterval(holen, 5_000);
    return () => window.clearInterval(id);
  }, [holen]);

  if (!paused) return null;

  const fortsetzen = () => {
    setBusy(true);
    setFehler(null);
    // Laufende Abfragen entwerten (s. `abfrage`).
    abfrage.current++;
    setAutoAssignPaused(false)
      .then(() => setPaused(false))
      .catch((e) => setFehler(String(e)))
      .finally(() => {
        setBusy(false);
        holen();
      });
  };

  return (
    <div className="flex items-center gap-2 bg-amber-500 px-4 py-2 text-sm font-medium text-white">
      <CirclePause size={16} className="shrink-0" />
      <span className="min-w-0">
        Automatik pausiert
        <span className="ml-1 font-normal opacity-90">
          — die automatische Feldvergabe belegt keine freien Felder.
          {fehler && ` Fortsetzen fehlgeschlagen: ${fehler}`}
        </span>
      </span>
      <button
        type="button"
        disabled={busy}
        onClick={fortsetzen}
        className="ml-auto shrink-0 rounded bg-white px-3 py-1 text-amber-700 transition-colors hover:bg-amber-50 disabled:opacity-60"
      >
        Fortsetzen
      </button>
    </div>
  );
}
