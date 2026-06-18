// Eigener Menüpunkt „Siegerehrung": steuert, welche ausgespielte Disziplin der
// Sieger-Monitor zeigt (bewusst NICHT rotierend — die Ehrung wird live
// gesteuert, damit das Publikum das Podium fotografieren kann). Die Zuweisung
// eines TVs auf „Siegerehrung" passiert weiterhin unter „Monitore".
import { Check, Info, Trophy, Tv } from "lucide-react";
import { useEffect, useState } from "react";
import { setWinnersSelection, winnersOverview } from "../api";
import type { DisciplineResult } from "../types";

// Kurzer Gold-Spieler-Vorschau-Text (Name + Verein) für die Auswahlliste.
function goldPreview(d: DisciplineResult): string {
  const gold = d.podium.find((p) => p.rank === 1);
  if (!gold) return "";
  const names = gold.players.map((x) => x.name).join(" / ");
  const club = gold.players
    .map((x) => x.club)
    .filter((c, i, a) => c && a.indexOf(c) === i)
    .join(" · ");
  return club ? `${names} (${club})` : names;
}

export function WinnersPage() {
  const [disciplines, setDisciplines] = useState<DisciplineResult[]>([]);
  const [selected, setSelected] = useState<number | null>(null);

  useEffect(() => {
    let active = true;
    const tick = () => {
      winnersOverview()
        .then((v) => {
          if (!active) return;
          setDisciplines(v.disciplines);
          setSelected(v.selected);
        })
        .catch(() => {});
    };
    tick();
    const id = setInterval(tick, 2000);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, []);

  // Auswahl sofort lokal spiegeln (responsives Gefühl), dann an den Kern.
  const choose = (drawId: number | null) => {
    setSelected(drawId);
    setWinnersSelection(drawId).catch(() => {});
  };

  return (
    <main className="mx-auto flex min-h-full max-w-4xl flex-col gap-5 p-6 text-slate-800">
      <header className="flex items-center gap-3">
        <div className="flex-1">
          <h1 className="text-2xl font-semibold leading-tight">Siegerehrung</h1>
          <p className="text-sm text-slate-500">
            Steuert, welches Podium auf dem Sieger-Monitor erscheint.
          </p>
        </div>
        <span className="inline-flex items-center gap-1.5 rounded-full bg-amber-100 px-3 py-1 text-xs font-medium text-amber-700">
          <Trophy size={14} />
          {selected == null ? "nichts live" : "live"}
        </span>
      </header>

      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-slate-700">Disziplin wählen</h2>
        <p className="text-xs text-slate-500">
          Wähle die Disziplin, die auf dem Sieger-Monitor erscheint. Es wird
          genau diese gezeigt (keine Rotation) – ideal zum Fotografieren des
          Podiums. Weise einem TV unter „Monitore" die „Siegerehrung" zu (ganzes
          Podium oder ein Monitor je Platz 1/2/3).
        </p>
        {disciplines.length === 0 ? (
          <div className="flex gap-2.5 rounded-xl border border-slate-200 bg-white p-4 text-sm text-slate-500 shadow-sm">
            <Info size={18} className="mt-0.5 shrink-0 text-slate-400" />
            <span>
              Noch keine Disziplin ausgespielt. Sobald ein Finale beendet ist,
              erscheint sie hier zur Auswahl.
            </span>
          </div>
        ) : (
          <div className="flex flex-col gap-1.5">
            <button
              type="button"
              onClick={() => choose(null)}
              className={`flex items-center justify-between rounded-lg border px-3 py-2 text-left text-sm ${
                selected == null
                  ? "border-amber-400 bg-amber-50 font-medium text-amber-900"
                  : "border-slate-200 bg-white text-slate-600 hover:bg-slate-50"
              }`}
            >
              <span>Nichts zeigen (Begrüßungsbild)</span>
              {selected == null && <Check size={16} className="text-amber-600" />}
            </button>
            {disciplines.map((d) => {
              const on = selected === d.draw_id;
              return (
                <button
                  key={d.draw_id}
                  type="button"
                  onClick={() => choose(d.draw_id)}
                  className={`flex items-center justify-between gap-3 rounded-lg border px-3 py-2 text-left text-sm ${
                    on
                      ? "border-amber-400 bg-amber-50 text-amber-900"
                      : "border-slate-200 bg-white text-slate-700 hover:bg-slate-50"
                  }`}
                >
                  <span className="min-w-0">
                    <span className="font-medium">{d.draw_name}</span>
                    {goldPreview(d) && (
                      <span className="ml-2 text-xs text-slate-500">
                        🥇 {goldPreview(d)}
                      </span>
                    )}
                  </span>
                  {on ? (
                    <span className="flex shrink-0 items-center gap-1 text-xs font-semibold text-amber-700">
                      <Tv size={14} /> live
                    </span>
                  ) : (
                    <span className="shrink-0 text-xs text-slate-400">zeigen</span>
                  )}
                </button>
              );
            })}
          </div>
        )}
      </section>
    </main>
  );
}
