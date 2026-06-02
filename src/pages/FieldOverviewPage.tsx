// Spielübersicht + Feldvergabe. Zeigt links die spielbereiten Matches und
// rechts die Felder als Ampel (grün=frei, gelb=belegt, rot=gesperrt). Ein Match
// wählen und auf ein freies Feld klicken → Zuweisung wird nach BTP geschrieben
// (bidirektional: beim nächsten Poll zeigen bts-light UND BTP dasselbe).
// Belegtes Feld → freigeben. Sperren-Umschalter je Feld.
import { type DragEvent, useCallback, useEffect, useRef, useState } from "react";
import { ArrowLeft, Ban, Lock, Unlock } from "lucide-react";
import {
  assignCourt,
  freeCourt,
  preparationCandidates,
  setCourtLocked,
  tabletOverview,
} from "../api";
import type { CourtOverview, PreparationCandidate } from "../types";

const POLL_MS = 2500;

function teamsLabel(t1: string[], t2: string[]): string {
  const a = t1.join(" / ") || "—";
  const b = t2.join(" / ") || "—";
  return `${a} – ${b}`;
}

export function FieldOverviewPage({ onBack }: { onBack: () => void }) {
  const [courts, setCourts] = useState<CourtOverview[]>([]);
  const [candidates, setCandidates] = useState<PreparationCandidate[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>("");
  const timer = useRef<number | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [info, prep] = await Promise.all([
        tabletOverview(),
        preparationCandidates(),
      ]);
      setCourts(info.courts);
      setCandidates(prep.candidates);
    } catch {
      /* Poll-Aussetzer tolerieren – letzter Stand bleibt stehen */
    }
  }, []);

  useEffect(() => {
    void refresh();
    timer.current = window.setInterval(() => void refresh(), POLL_MS);
    return () => {
      if (timer.current) window.clearInterval(timer.current);
    };
  }, [refresh]);

  // Eine BTP-schreibende Aktion ausführen, dann sofort neu laden.
  async function run(action: () => Promise<void>) {
    setBusy(true);
    setError("");
    try {
      await action();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  // Ein Match (per Klick-Auswahl oder Drag&Drop) einem freien Feld zuweisen.
  function assignTo(matchId: number, c: CourtOverview) {
    if (busy || c.locked || c.match_id > 0) return;
    // Auswahl erst nach erfolgreicher Zuweisung leeren – schlägt der BTP-Write
    // fehl, bleibt das Spiel gewählt und der Klick/Drop lässt sich wiederholen.
    void run(async () => {
      await assignCourt(matchId, c.court_id);
      setSelected(null);
    });
  }

  function onCourtClick(c: CourtOverview) {
    if (busy || c.locked || c.match_id > 0) return;
    if (selected == null) {
      setError("Erst links ein Spiel wählen (oder es auf ein Feld ziehen).");
      return;
    }
    assignTo(selected, c);
  }

  function onCourtDrop(e: DragEvent, c: CourtOverview) {
    e.preventDefault();
    const matchId = Number(e.dataTransfer.getData("text/plain"));
    if (matchId) assignTo(matchId, c);
  }

  // Bereits auf einem Feld stehende Matches nicht in der Auswahl-Liste, aber
  // separat „Auf Feld" farblich markiert anzeigen (gewünschter Überblick).
  const onCourtMatchIds = new Set(courts.map((c) => c.match_id).filter((id) => id > 0));
  const assignable = candidates.filter((m) => !onCourtMatchIds.has(m.match_id));
  const onField = courts.filter((c) => c.match_id > 0);

  return (
    <main className="mx-auto flex min-h-full max-w-5xl flex-col gap-5 p-6 text-slate-800">
      <header className="flex items-center gap-3">
        <button
          onClick={onBack}
          className="inline-flex items-center gap-1.5 rounded-lg bg-slate-100 px-3 py-2
                     text-sm font-medium text-slate-700 hover:bg-slate-200"
        >
          <ArrowLeft size={16} /> Zurück
        </button>
        <div>
          <h1 className="text-2xl font-semibold leading-tight">Spielübersicht</h1>
          <p className="text-sm text-slate-500">
            Spiele auf Felder zuweisen, freigeben, sperren — schreibt nach BTP.
          </p>
        </div>
      </header>

      {error && (
        <div className="rounded-xl border border-rose-200 bg-rose-50 px-4 py-2.5 text-sm text-rose-800">
          {error}
        </div>
      )}

      <div className="grid gap-5 md:grid-cols-[20rem_1fr]">
        {/* Spalte links: spielbereite Matches */}
        <section className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-slate-700">
            Spielbereit <span className="text-slate-400">({assignable.length})</span>
          </h2>
          <p className="text-xs text-slate-500">
            Spiel auf ein freies (grünes) Feld <strong>ziehen</strong> — oder
            anklicken und dann aufs Feld klicken.
          </p>
          <div className="flex flex-col gap-1.5">
            {assignable.length === 0 && (
              <div className="rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-400">
                Keine spielbereiten Spiele.
              </div>
            )}
            {assignable.map((m) => {
              const active = selected === m.match_id;
              return (
                <button
                  key={m.match_id}
                  draggable
                  onDragStart={(e) => {
                    e.dataTransfer.setData("text/plain", String(m.match_id));
                    e.dataTransfer.effectAllowed = "move";
                  }}
                  onClick={() => setSelected(active ? null : m.match_id)}
                  className={`cursor-grab rounded-lg border px-3 py-2 text-left text-sm transition-colors active:cursor-grabbing ${
                    active
                      ? "border-slate-800 bg-slate-800 text-white"
                      : "border-slate-200 bg-white hover:border-slate-400"
                  }`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-medium">{m.label || "Spiel"}</span>
                    {m.match_num != null && (
                      <span className={active ? "text-slate-300" : "text-slate-400"}>
                        #{m.match_num}
                      </span>
                    )}
                  </div>
                  <div className={`text-xs ${active ? "text-slate-200" : "text-slate-500"}`}>
                    {teamsLabel(m.team1, m.team2)}
                  </div>
                </button>
              );
            })}
          </div>

          {/* Auf Feld stehende Spiele – farblich markiert (gelb wie belegt). */}
          {onField.length > 0 && (
            <>
              <h2 className="mt-3 text-sm font-semibold text-slate-700">
                Auf Feld <span className="text-slate-400">({onField.length})</span>
              </h2>
              <div className="flex flex-col gap-1.5">
                {onField.map((c) => (
                  <div
                    key={c.court_id}
                    className="rounded-lg border border-amber-300 bg-amber-50 px-3 py-2 text-sm"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="font-medium text-amber-900">
                        {c.match_name || "Spiel"}
                      </span>
                      <span className="shrink-0 rounded-full bg-amber-200 px-2 py-0.5 text-xs font-semibold text-amber-900">
                        Feld {c.court}
                      </span>
                    </div>
                    <div className="text-xs text-slate-600">
                      {teamsLabel(c.team1, c.team2)}
                    </div>
                  </div>
                ))}
              </div>
            </>
          )}
        </section>

        {/* Spalte rechts: Felder als Ampel */}
        <section className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-slate-700">Felder</h2>
          <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-3">
            {courts.map((c) => {
              const occupied = c.match_id > 0;
              const cls = c.locked
                ? "border-rose-300 bg-rose-50"
                : occupied
                  ? "border-amber-300 bg-amber-50"
                  : "border-emerald-300 bg-emerald-50";
              const clickable = !c.locked && !occupied && !busy;
              return (
                <div
                  key={c.court_id}
                  onClick={() => onCourtClick(c)}
                  // Freies, nicht gesperrtes Feld = Drop-Ziel für ein gezogenes Spiel.
                  onDragOver={(e) => {
                    if (clickable) e.preventDefault();
                  }}
                  onDrop={(e) => clickable && onCourtDrop(e, c)}
                  className={`relative rounded-xl border p-3 ${cls} ${
                    clickable ? "cursor-pointer hover:shadow-sm" : ""
                  }`}
                >
                  <div className="flex items-start justify-between gap-2">
                    <div className="font-semibold">
                      {c.court}
                      {c.location && (
                        <span className="ml-1 text-xs font-normal text-slate-400">
                          {c.location}
                        </span>
                      )}
                    </div>
                    <button
                      disabled={busy}
                      onClick={(e) => {
                        e.stopPropagation();
                        void run(() => setCourtLocked(c.court_id, !c.locked));
                      }}
                      title={c.locked ? "Feld entsperren" : "Feld sperren"}
                      className="rounded p-1 text-slate-500 hover:bg-white/60 disabled:opacity-50"
                    >
                      {c.locked ? <Lock size={15} /> : <Unlock size={15} />}
                    </button>
                  </div>

                  {c.locked ? (
                    <div className="mt-2 inline-flex items-center gap-1 text-xs font-medium text-rose-700">
                      <Ban size={13} /> Gesperrt
                    </div>
                  ) : occupied ? (
                    <div className="mt-1">
                      <div className="text-xs font-medium text-amber-800">
                        {c.match_name}
                      </div>
                      <div className="truncate text-xs text-slate-600" title={teamsLabel(c.team1, c.team2)}>
                        {teamsLabel(c.team1, c.team2)}
                      </div>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          void run(() => freeCourt(c.court_id));
                        }}
                        disabled={busy}
                        className="mt-2 rounded-md bg-amber-200/70 px-2.5 py-1 text-xs font-medium
                                   text-amber-900 hover:bg-amber-200 disabled:opacity-50"
                      >
                        Freigeben
                      </button>
                    </div>
                  ) : (
                    <div className="mt-2 text-xs text-emerald-700">
                      frei{selected != null ? " · klicken zum Zuweisen" : ""}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
          {courts.length === 0 && (
            <p className="text-sm text-slate-400">
              Keine Felder — läuft der Sync und ist ein Turnier in BTP geladen?
            </p>
          )}
        </section>
      </div>
    </main>
  );
}
