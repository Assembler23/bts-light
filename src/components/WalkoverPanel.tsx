import { useEffect, useState } from "react";
import { Flag, X } from "lucide-react";
import { confirmWalkover, dismissWalkover, walkoverProposals } from "../api";
import type { WalkoverProposal } from "../types";

/**
 * App-weites Modal nach einer Aufgabe: Gibt eine Mannschaft auf, hat aber
 * in derselben Disziplin noch weitere Spiele, schlägt bts-light vor, diese
 * kampflos (Walkover) für den jeweiligen Gegner zu werten. Die
 * Turnierleitung wählt die Spiele aus und bestätigt – erst dann wird nach
 * BTP geschrieben. Pollt die Vorschläge alle 4 s.
 */
export function WalkoverPanel() {
  const [proposals, setProposals] = useState<WalkoverProposal[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [checked, setChecked] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    const tick = () => {
      walkoverProposals()
        .then((p) => {
          if (alive) setProposals(p);
        })
        .catch(() => {});
    };
    tick();
    const id = setInterval(tick, 4000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);

  // Immer den ältesten offenen Vorschlag zeigen.
  const proposal = proposals[0] ?? null;

  // Bei einem neuen Vorschlag alle Spiele vorauswählen.
  useEffect(() => {
    if (proposal && proposal.id !== activeId) {
      setActiveId(proposal.id);
      setChecked(new Set(proposal.candidates.map((c) => c.match_id)));
      setError(null);
    }
  }, [proposal, activeId]);

  if (!proposal) return null;

  const toggle = (id: number) => {
    setChecked((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const refresh = () => walkoverProposals().then(setProposals).catch(() => {});

  const confirm = async () => {
    setBusy(true);
    setError(null);
    try {
      const res = await confirmWalkover(proposal.id, [...checked]);
      if (res.errors.length > 0) {
        setError(
          `${res.written} gewertet, ${res.errors.length} fehlgeschlagen: ${res.errors.join("; ")}`,
        );
      }
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const dismiss = async () => {
    setBusy(true);
    await dismissWalkover(proposal.id).catch(() => {});
    setProposals((prev) => prev.filter((p) => p.id !== proposal.id));
    setBusy(false);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/60 p-4">
      <div className="w-full max-w-md overflow-hidden rounded-xl bg-white shadow-xl">
        <div className="flex items-center gap-2 border-b border-slate-200 px-5 py-3">
          <Flag size={18} className="shrink-0 text-amber-600" />
          <h2 className="flex-1 font-semibold text-slate-800">
            Aufgabe – kampflose Wertung
          </h2>
          <button
            onClick={dismiss}
            disabled={busy}
            className="text-slate-400 transition-colors hover:text-slate-600 disabled:opacity-50"
            title="Schließen"
          >
            <X size={18} />
          </button>
        </div>

        <div className="px-5 py-4 text-sm text-slate-700">
          <p>
            <b>{proposal.retired_team}</b> hat in der Disziplin{" "}
            <b>{proposal.draw_name}</b> aufgegeben und hat dort noch weitere
            Spiele. Diese können kampflos für den Gegner gewertet werden:
          </p>
          <ul className="mt-3 flex flex-col gap-1.5">
            {proposal.candidates.map((c) => (
              <li key={c.match_id}>
                <label className="flex cursor-pointer items-center gap-2.5 rounded-lg border border-slate-200 px-3 py-2 transition-colors hover:bg-slate-50">
                  <input
                    type="checkbox"
                    checked={checked.has(c.match_id)}
                    onChange={() => toggle(c.match_id)}
                    className="size-4 accent-amber-600"
                  />
                  <span className="flex-1">
                    <span className="font-medium">
                      {c.round_name || "Spiel"}
                    </span>{" "}
                    <span className="text-slate-500">gegen</span> {c.opponent}
                  </span>
                </label>
              </li>
            ))}
          </ul>
          {error && (
            <p className="mt-3 rounded-lg bg-rose-50 px-3 py-2 text-rose-700">
              {error}
            </p>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-slate-200 bg-slate-50 px-5 py-3">
          <button
            onClick={dismiss}
            disabled={busy}
            className="rounded-lg px-3 py-1.5 text-sm font-medium text-slate-600 transition-colors hover:bg-slate-200 disabled:opacity-50"
          >
            Nicht werten
          </button>
          <button
            onClick={confirm}
            disabled={busy || checked.size === 0}
            className="rounded-lg bg-amber-600 px-3 py-1.5 text-sm font-medium text-white transition-colors hover:bg-amber-700 disabled:opacity-50"
          >
            {busy
              ? "Wird gewertet …"
              : `${checked.size} Spiel${checked.size === 1 ? "" : "e"} kampflos werten`}
          </button>
        </div>
      </div>
    </div>
  );
}
