// Spielübersicht + Feldvergabe als Board: oben der Pool spielbereiter Spiele,
// darunter die Felder als Spalten (Ampel: grün=frei, gelb=belegt, rot=gesperrt)
// mit Aufruf-Uhr. Spiel auf eine freie Spalte ziehen (oder anklicken + Spalte)
// → Zuweisung wird nach BTP geschrieben (bidirektional). Belegtes Feld →
// freigeben (mit Sicherheitsabfrage). Sperren-Umschalter je Feld. Bei ≥2 Hallen
// nach Halle gruppiert + Hallen-Filter.
import { type DragEvent, useCallback, useEffect, useRef, useState } from "react";
import { Ban, Lock, Megaphone, Unlock } from "lucide-react";
import {
  assignCourt,
  finishedMatches,
  freeCourt,
  preparationCandidates,
  setCourtLocked,
  tabletOverview,
} from "../api";
import { CallTimerBadge } from "../components/CallTimerBadge";
import { HallFilter } from "../components/HallFilter";
import { announceCourt } from "../io/announceCourt";
import { useNow } from "../state/callTimer";
import type {
  AnnounceConfig,
  AzureTtsConfig,
  CallTimerConfig,
  CourtOverview,
  DisciplineHallRule,
  FinishedMatchRow,
  PreparationCandidate,
} from "../types";

const POLL_MS = 2500;

function teamsLabel(t1: string[], t2: string[]): string {
  const a = t1.join(" / ") || "—";
  const b = t2.join(" / ") || "—";
  return `${a} – ${b}`;
}

// BTP-PlannedTime (YYYYMMDDHHMM) → „HH:MM"; leer ohne Ansetzung.
function fmtPlannedTime(t: number | null): string {
  if (!t) return "";
  const hh = Math.floor((t / 100) % 100);
  const mm = t % 100;
  return `${String(hh).padStart(2, "0")}:${String(mm).padStart(2, "0")}`;
}

// Satz-Ergebnisse → „15:9 11:15 14:16".
function fmtSets(sets: [number, number][]): string {
  return sets.map(([a, b]) => `${a}:${b}`).join("  ");
}

// „Spiel"-Spalte: Zeit · Klasse · Runde (leere Teile weggelassen).
function spielLabel(
  time: number | null,
  draw: string,
  round: string,
): string {
  return [fmtPlannedTime(time), draw, round].filter(Boolean).join(" ");
}

export function FieldOverviewPage({
  callTimer,
  announce,
  azureTts,
  disciplineHallRules,
}: {
  callTimer: CallTimerConfig;
  announce: AnnounceConfig;
  azureTts?: AzureTtsConfig;
  /** Disziplin/Klasse→Halle-Regeln (Mehr-Hallen): nicht erlaubte Felder werden
   *  ausgegraut; eine Vergabe dorthin wird abgewiesen (Backend erzwingt es). */
  disciplineHallRules: DisciplineHallRule[];
}) {
  const [courts, setCourts] = useState<CourtOverview[]>([]);
  const [candidates, setCandidates] = useState<PreparationCandidate[]>([]);
  const [finished, setFinished] = useState<FinishedMatchRow[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>("");
  // Feld, dessen Freigabe gerade bestätigt werden soll (Sicherheitsabfrage).
  const [confirmFree, setConfirmFree] = useState<CourtOverview | null>(null);
  // Hallen-Filter (null = alle Hallen).
  const [hallFilter, setHallFilter] = useState<string | null>(null);
  const timer = useRef<number | null>(null);
  const now = useNow();

  const refresh = useCallback(async () => {
    try {
      const [info, prep, fin] = await Promise.all([
        tabletOverview(),
        preparationCandidates(),
        finishedMatches(),
      ]);
      setCourts(info.courts);
      setCandidates(prep.candidates);
      // Nur bei echter Änderung neu setzen → die (wachsende) Tabelle rendert
      // nicht bei jedem 2,5-s-Poll neu.
      setFinished((prev) =>
        prev.length === fin.length &&
        prev.every(
          (r, i) =>
            r.match_id === fin[i].match_id &&
            r.finished_at === fin[i].finished_at,
        )
          ? prev
          : fin,
      );
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

  // Disziplin/Klasse→Halle: erlaubte Halle eines Matches (oder null = frei).
  // Spiegelt die Backend-Regel (config::AppConfig::allowed_hall_for): exakte
  // Auslosung (draw_name) schlägt den Kategorie-Default.
  function allowedHallForMatch(m: PreparationCandidate | undefined): string | null {
    if (!m) return null;
    const dn = (m.draw_name || "").trim().toLowerCase();
    if (dn) {
      const cls = disciplineHallRules.find(
        (r) =>
          r.discipline === m.discipline &&
          r.draw_name.trim() !== "" &&
          r.draw_name.trim().toLowerCase() === dn &&
          r.hall.trim() !== "",
      );
      if (cls) return cls.hall.trim();
    }
    const cat = disciplineHallRules.find(
      (r) =>
        r.draw_name.trim() === "" &&
        r.discipline === m.discipline &&
        r.hall.trim() !== "",
    );
    return cat ? cat.hall.trim() : null;
  }
  function courtAllowedFor(
    m: PreparationCandidate | undefined,
    c: CourtOverview,
  ): boolean {
    const allowed = allowedHallForMatch(m);
    if (!allowed) return true;
    return (c.location || "").trim().toLowerCase() === allowed.toLowerCase();
  }

  // Ein Match (per Klick-Auswahl oder Drag&Drop) einem freien Feld zuweisen.
  function assignTo(matchId: number, c: CourtOverview) {
    if (busy || c.locked || c.match_id > 0) return;
    // Spiel könnte seit dem Auswählen/Ziehen aus den spielbereiten gefallen
    // sein (anderer Operator, BTP-Wechsel) → dann nicht blind nach BTP schreiben.
    const cand = candidates.find((m) => m.match_id === matchId);
    if (!cand) {
      setSelected(null);
      setError("Das Spiel ist nicht mehr spielbereit — bitte neu wählen.");
      return;
    }
    // Disziplin/Klasse→Halle-Regel: Vergabe in die falsche Halle früh abweisen
    // (das Backend erzwingt es zusätzlich). Auswahl bleibt → anderes Feld wählbar.
    if (!courtAllowedFor(cand, c)) {
      setError(
        `„${cand.draw_name || cand.label}" darf nur in Halle „${allowedHallForMatch(cand)}" vergeben werden — dieses Feld liegt in „${c.location || "—"}".`,
      );
      return;
    }
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
      setError("Erst oben ein Spiel wählen (oder es auf eine Feld-Spalte ziehen).");
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
  // Anzeige im Bestätigungs-Dialog stets aus dem Live-Stand des Felds ziehen
  // (über die stabile court_id), damit sie bei einem Poll-Wechsel nicht veraltet.
  const liveConfirm = confirmFree
    ? courts.find((c) => c.court_id === confirmFree.court_id) ?? confirmFree
    : null;
  // Aktuell gewähltes Spiel (für das Ausgrauen nicht erlaubter Hallen-Felder).
  const selCand =
    selected != null
      ? candidates.find((m) => m.match_id === selected)
      : undefined;

  // Felder als Board-Spalten, bei ≥2 Hallen nach Halle gruppiert.
  const allHalls = [
    ...new Set(courts.map((c) => c.location).filter((l) => l !== "")),
  ].sort((a, b) => a.localeCompare(b, "de"));
  const multiHall = allHalls.length >= 2;
  const hallGroups: { hall: string; courts: CourtOverview[] }[] = multiHall
    ? [
        ...allHalls.map((h) => ({
          hall: h,
          courts: courts.filter((c) => c.location === h),
        })),
        // Felder ohne Halle ans Ende.
        { hall: "", courts: courts.filter((c) => c.location === "") },
      ].filter((g) => g.courts.length > 0)
    : [{ hall: "", courts }];
  const visibleGroups =
    hallFilter === null
      ? hallGroups
      : hallGroups.filter((g) => g.hall === hallFilter);

  return (
    <main className="mx-auto flex min-h-full max-w-6xl flex-col gap-4 p-6 text-slate-800">
      <header>
        <h1 className="text-2xl font-semibold leading-tight">Spielübersicht</h1>
        <p className="text-sm text-slate-500">
          Oben die Felder, darunter die nicht zugewiesenen und die
          abgeschlossenen Spiele. Ein offenes Spiel auf ein freies Feld ziehen
          (oder anklicken + Feld tippen) → schreibt nach BTP.
        </p>
      </header>

      {error && (
        <div className="rounded-xl border border-rose-200 bg-rose-50 px-4 py-2.5 text-sm text-rose-800">
          {error}
        </div>
      )}

      {/* Felder oben: Board (Drop-Ziel für die offenen Spiele unten). */}
      <HallFilter halls={allHalls} value={hallFilter} onChange={setHallFilter} />

      {/* Board: Felder als Spalten, je Halle eine Gruppe. */}
      {courts.length === 0 ? (
        <p className="text-sm text-slate-400">
          Keine Felder — läuft der Sync und ist ein Turnier in BTP geladen?
        </p>
      ) : (
        visibleGroups.map((g) => (
          <section key={g.hall || "_"} className="flex flex-col gap-2">
            {multiHall && (
              <h2 className="text-sm font-semibold text-slate-600">
                {g.hall || "Ohne Halle"}
              </h2>
            )}
            <div className="flex flex-wrap gap-2.5">
              {g.courts.map((c) => {
                const occupied = c.match_id > 0;
                const clickable = !c.locked && !occupied && !busy;
                // Disziplin/Klasse→Halle: freies Feld, das fürs gewählte Spiel
                // nicht erlaubt ist → ausgrauen (Klick zeigt trotzdem die
                // Begründung; das Backend erzwingt die Regel ohnehin).
                const blockedByHall =
                  clickable && selCand ? !courtAllowedFor(selCand, c) : false;
                // Ampel: Kopfzeile farbig je Status.
                const head = c.locked
                  ? "bg-rose-100 text-rose-800"
                  : occupied
                    ? "bg-amber-100 text-amber-900"
                    : "bg-emerald-100 text-emerald-800";
                return (
                  <div
                    key={c.court_id}
                    onClick={() => onCourtClick(c)}
                    onDragOver={(e) => {
                      if (clickable && !blockedByHall) e.preventDefault();
                    }}
                    onDrop={(e) =>
                      clickable && !blockedByHall && onCourtDrop(e, c)
                    }
                    title={
                      blockedByHall
                        ? `Für „${selCand?.draw_name || selCand?.label}" nicht erlaubt (andere Halle)`
                        : undefined
                    }
                    className={`flex w-44 flex-col overflow-hidden rounded-xl border border-slate-200 bg-white ${
                      clickable ? "cursor-pointer hover:border-slate-400 hover:shadow-sm" : ""
                    } ${blockedByHall ? "opacity-40" : ""}`}
                  >
                    {/* Spaltenkopf: Feldname + Ampelpunkt + Sperren-Schalter. */}
                    <div className={`flex items-center justify-between gap-1 px-2.5 py-1.5 ${head}`}>
                      <span className="flex items-center gap-1.5 font-semibold">
                        <span
                          className={`h-2 w-2 rounded-full ${
                            c.locked ? "bg-rose-500" : occupied ? "bg-amber-500" : "bg-emerald-500"
                          }`}
                        />
                        Feld {c.court}
                      </span>
                      <button
                        disabled={busy}
                        onClick={(e) => {
                          e.stopPropagation();
                          void run(() => setCourtLocked(c.court_id, !c.locked));
                        }}
                        title={c.locked ? "Feld entsperren" : "Feld sperren"}
                        className="rounded p-0.5 hover:bg-white/50 disabled:opacity-50"
                      >
                        {c.locked ? <Lock size={14} /> : <Unlock size={14} />}
                      </button>
                    </div>

                    {/* Spalteninhalt je Status. */}
                    <div className="flex min-h-[5.5rem] flex-col gap-1 p-2.5">
                      {c.locked ? (
                        <div className="inline-flex items-center gap-1 text-xs font-medium text-rose-700">
                          <Ban size={13} /> Gesperrt
                        </div>
                      ) : occupied ? (
                        <>
                          <div className="text-xs font-medium text-amber-800">
                            {c.match_name}
                          </div>
                          <div
                            className="text-xs text-slate-600"
                            title={teamsLabel(c.team1, c.team2)}
                          >
                            {teamsLabel(c.team1, c.team2)}
                          </div>
                          {/* Tabletbediener (= „Schiedsrichter"-Spalte, bis das
                              Schiri-Modul echte Schiris liefert). */}
                          {c.scorekeeper.length > 0 && (
                            <div className="truncate text-[11px] text-slate-400">
                              Bediener: {c.scorekeeper.join(" / ")}
                            </div>
                          )}
                          {callTimer.enabled && c.on_court_since_ms != null && (
                            <CallTimerBadge
                              sinceMs={c.on_court_since_ms}
                              now={now}
                              cfg={callTimer}
                            />
                          )}
                          <div className="mt-auto flex items-center gap-1.5">
                            {/* „Nochmal aufrufen" nur, wenn Ansagen aktiviert
                                sind – sonst gibt es keine Sprachausgabe. */}
                            {announce.enabled && (
                              <button
                                onClick={(e) => {
                                  e.stopPropagation();
                                  announceCourt(c, announce, azureTts);
                                }}
                                disabled={busy}
                                aria-label={`Feld ${c.court} nochmal aufrufen`}
                                title="Dieses Feld nochmal aufrufen (Ansage)"
                                className="inline-flex items-center gap-1 rounded-md bg-slate-100 px-2 py-1
                                           text-xs font-medium text-slate-700 hover:bg-slate-200 disabled:opacity-50"
                              >
                                <Megaphone size={13} /> Aufrufen
                              </button>
                            )}
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                setConfirmFree(c);
                              }}
                              disabled={busy}
                              className="rounded-md bg-amber-200/70 px-2.5 py-1 text-xs font-medium
                                         text-amber-900 hover:bg-amber-200 disabled:opacity-50"
                            >
                              Freigeben
                            </button>
                          </div>
                        </>
                      ) : (
                        <div className="flex flex-1 items-center justify-center text-center text-xs text-emerald-700">
                          {selected != null ? "klicken/ziehen zum Zuweisen" : "frei"}
                        </div>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </section>
        ))
      )}
      {/* Hallen-Filter aktiv, aber keine Felder in dieser Halle. */}
      {courts.length > 0 && visibleGroups.length === 0 && (
        <p className="text-sm text-slate-400">
          Keine Felder in „{hallFilter}". Über „Alle" siehst du alle Felder.
        </p>
      )}

      {/* Nicht zugewiesene Spiele: per Drag&Drop (oder Klick) auf ein Feld. */}
      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-slate-700">
          Nicht zugewiesene Spiele{" "}
          <span className="text-slate-400">({assignable.length})</span>
        </h2>
        {assignable.length === 0 ? (
          <p className="text-sm text-slate-400">Keine spielbereiten Spiele.</p>
        ) : (
          <div className="overflow-x-auto rounded-xl border border-slate-200">
            <table className="w-full text-sm">
              <thead className="bg-slate-50 text-left text-xs text-slate-500">
                <tr>
                  <th className="px-3 py-2 font-medium">#</th>
                  <th className="px-3 py-2 font-medium">Spiel</th>
                  <th className="px-3 py-2 font-medium">Spieler</th>
                  <th className="px-3 py-2 font-medium">Halle</th>
                </tr>
              </thead>
              <tbody>
                {assignable.map((m) => {
                  const active = selected === m.match_id;
                  const hall = allowedHallForMatch(m);
                  return (
                    <tr
                      key={m.match_id}
                      draggable
                      onDragStart={(e) => {
                        e.dataTransfer.setData("text/plain", String(m.match_id));
                        e.dataTransfer.effectAllowed = "move";
                      }}
                      onClick={() => setSelected(active ? null : m.match_id)}
                      className={`cursor-grab border-t border-slate-100 active:cursor-grabbing ${
                        active ? "bg-slate-800 text-white" : "hover:bg-slate-50"
                      }`}
                    >
                      <td className="px-3 py-2 tabular-nums">
                        {m.match_num ?? ""}
                      </td>
                      <td className="px-3 py-2">
                        {spielLabel(m.planned_time, m.draw_name, m.round_name) ||
                          m.label}
                      </td>
                      <td className="px-3 py-2">
                        {teamsLabel(m.team1, m.team2)}
                      </td>
                      <td className="px-3 py-2">
                        {hall ? (
                          <span
                            className={`rounded px-1.5 py-0.5 text-xs ${
                              active
                                ? "bg-white/20"
                                : "bg-violet-100 text-violet-800"
                            }`}
                          >
                            {hall}
                          </span>
                        ) : (
                          <span
                            className={active ? "text-slate-300" : "text-slate-400"}
                          >
                            —
                          </span>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
        <p className="text-xs text-slate-400">
          Zeile auf ein freies (grünes) Feld oben ziehen — oder anklicken und
          dann auf die Feld-Spalte tippen. „Halle" = durch die Disziplin-Regel
          vorgegebene Halle.
        </p>
      </section>

      {/* Abgeschlossene Spiele (neueste zuerst). */}
      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-slate-700">
          Abgeschlossene Spiele{" "}
          <span className="text-slate-400">({finished.length})</span>
        </h2>
        {finished.length === 0 ? (
          <p className="text-sm text-slate-400">
            Noch keine abgeschlossenen Spiele.
          </p>
        ) : (
          <div className="overflow-x-auto rounded-xl border border-slate-200">
            <table className="w-full text-sm">
              <thead className="bg-slate-50 text-left text-xs text-slate-500">
                <tr>
                  <th className="px-3 py-2 font-medium">Feld</th>
                  <th className="px-3 py-2 font-medium">#</th>
                  <th className="px-3 py-2 font-medium">Spiel</th>
                  <th className="px-3 py-2 font-medium">Spieler</th>
                  <th className="px-3 py-2 font-medium">Schiedsrichter</th>
                  <th className="px-3 py-2 font-medium">Ergebnis</th>
                </tr>
              </thead>
              <tbody>
                {finished.map((m) => {
                  const t1 = m.team1.join(" / ") || "—";
                  const t2 = m.team2.join(" / ") || "—";
                  const fieldLabel =
                    [m.location, m.court].filter(Boolean).join(" · ") || "—";
                  const resultLabel: Record<string, string> = {
                    walkover: "kampflos",
                    retired: "Aufgabe",
                    disqualified: "DQ",
                  };
                  return (
                    <tr
                      key={m.match_id}
                      className="border-t border-slate-100 hover:bg-slate-50"
                    >
                      <td className="px-3 py-2">{fieldLabel}</td>
                      <td className="px-3 py-2 tabular-nums">
                        {m.match_num ?? ""}
                      </td>
                      <td className="px-3 py-2">
                        {spielLabel(m.planned_time, m.draw_name, m.round_name) ||
                          m.draw_name ||
                          "—"}
                      </td>
                      <td className="px-3 py-2">
                        <span className={m.winner === 1 ? "font-semibold" : ""}>
                          {t1}
                        </span>
                        <span className="text-slate-400"> – </span>
                        <span className={m.winner === 2 ? "font-semibold" : ""}>
                          {t2}
                        </span>
                      </td>
                      {/* Tabletbediener/Schiri liegt je abgeschlossenem Spiel noch
                          nicht vor (kommt mit dem Schiri-Modul). */}
                      <td className="px-3 py-2 text-slate-300">—</td>
                      <td className="px-3 py-2 tabular-nums">
                        {fmtSets(m.sets)}
                        {resultLabel[m.result] && (
                          <span className="ml-1.5 rounded bg-slate-100 px-1 py-0.5 text-xs text-slate-500">
                            {resultLabel[m.result]}
                          </span>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </section>

      {/* Sicherheitsabfrage vor dem Freigeben eines belegten Felds. */}
      {confirmFree && liveConfirm && (
        <div
          role="dialog"
          aria-modal="true"
          aria-labelledby="free-confirm-title"
          className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/60 p-4"
        >
          <div className="w-full max-w-md overflow-hidden rounded-xl bg-white shadow-xl">
            <div className="border-b border-slate-200 px-5 py-3">
              <h2 id="free-confirm-title" className="font-semibold text-slate-800">
                Feld {liveConfirm.court} freigeben?
              </h2>
            </div>
            <div className="px-5 py-4 text-sm text-slate-700">
              <p>
                Das Feld wird in BTP zurückgezogen — Halle und Feld werden am
                Spiel entfernt.
              </p>
              <p className="mt-2 font-medium text-rose-700">
                Achtung: Läuft auf dem Feld ein Spiel, wird der laufende
                Spielstand verworfen.
              </p>
              <p className="mt-2 text-slate-600">
                {liveConfirm.match_name || "Spiel"} —{" "}
                {teamsLabel(liveConfirm.team1, liveConfirm.team2)}
              </p>
            </div>
            <div className="flex justify-end gap-2 border-t border-slate-200 bg-slate-50 px-5 py-3">
              <button
                onClick={() => setConfirmFree(null)}
                disabled={busy}
                className="rounded-lg bg-slate-100 px-3.5 py-2 text-sm font-medium
                           text-slate-700 transition-colors hover:bg-slate-200 disabled:opacity-50"
              >
                Abbrechen
              </button>
              <button
                onClick={() => {
                  const court_id = confirmFree.court_id;
                  setConfirmFree(null);
                  void run(() => freeCourt(court_id));
                }}
                disabled={busy}
                className="rounded-lg bg-rose-600 px-3.5 py-2 text-sm font-medium text-white
                           transition-colors hover:bg-rose-700 disabled:opacity-50"
              >
                Freigeben
              </button>
            </div>
          </div>
        </div>
      )}
    </main>
  );
}
