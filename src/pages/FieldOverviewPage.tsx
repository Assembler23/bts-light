// Spielübersicht + Feldvergabe als Board: oben der Pool spielbereiter Spiele,
// darunter die Felder als Spalten (Ampel: grün=frei, gelb=belegt, rot=gesperrt)
// mit Aufruf-Uhr. Spiel auf eine freie Spalte ziehen (oder anklicken + Spalte)
// → Zuweisung wird nach BTP geschrieben (bidirektional). Belegtes Feld →
// freigeben (mit Sicherheitsabfrage). Sperren-Umschalter je Feld. Bei ≥2 Hallen
// nach Halle gruppiert + Hallen-Filter.
import {
  type DragEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { Ban, Lock, Megaphone, Unlock } from "lucide-react";
import {
  addScorekeeper,
  advanceScorekeeper,
  assignCourt,
  disqualifyMatch,
  enterResult,
  finishedMatches,
  freeCourt,
  preparationCandidates,
  removeScorekeeper,
  scorekeeperQueue,
  setCourtLocked,
  tabletOverview,
} from "../api";
import { CallTimerBadge } from "../components/CallTimerBadge";
import { HallFilter } from "../components/HallFilter";
import { announceCourt } from "../io/announceCourt";
import { gamePointKind } from "../io/gamePoint.mjs";
import { useNow } from "../state/callTimer";
import type {
  AnnounceConfig,
  AzureTtsConfig,
  CallTimerConfig,
  CourtOverview,
  DisciplineHallRule,
  FinishedMatchRow,
  PreparationCandidate,
  ScorekeeperEntry,
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
function spielLabel(time: number | null, draw: string, round: string): string {
  return [fmtPlannedTime(time), draw, round].filter(Boolean).join(" ");
}

export function FieldOverviewPage({
  callTimer,
  announce,
  azureTts,
  disciplineHallRules,
  manageScorekeepers,
}: {
  callTimer: CallTimerConfig;
  announce: AnnounceConfig;
  azureTts?: AzureTtsConfig;
  /** Disziplin/Klasse→Halle-Regeln (Mehr-Hallen): nicht erlaubte Felder werden
   *  ausgegraut; eine Vergabe dorthin wird abgewiesen (Backend erzwingt es). */
  disciplineHallRules: DisciplineHallRule[];
  /** Zähltafelbediener-Warteschlange führen (ADR 0007, Config-Schalter). */
  manageScorekeepers: boolean;
}) {
  const [courts, setCourts] = useState<CourtOverview[]>([]);
  const [candidates, setCandidates] = useState<PreparationCandidate[]>([]);
  const [finished, setFinished] = useState<FinishedMatchRow[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>("");
  // Feld, dessen Freigabe gerade bestätigt werden soll (Sicherheitsabfrage).
  const [confirmFree, setConfirmFree] = useState<CourtOverview | null>(null);
  // Backend-Finalisierung (Plan 12): Feld, für dessen Spiel die
  // Turnierleitung gerade ein Ergebnis eintippt, plus die editierbaren Sätze.
  const [enterFor, setEnterFor] = useState<CourtOverview | null>(null);
  const [enterSets, setEnterSets] = useState<[number, number][]>([]);
  // Zwei-Schritt-Bestätigung der Disqualifikation (folgenreich): erst Team
  // wählen, dann bestätigen. null = keine Auswahl offen.
  const [dqConfirm, setDqConfirm] = useState<1 | 2 | null>(null);
  // Zähltafelbediener-Warteschlange (ADR 0007) + Eingabe fürs manuelle Hinzufügen.
  const [skQueue, setSkQueue] = useState<ScorekeeperEntry[]>([]);
  const [skAdd, setSkAdd] = useState("");
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

  // Zähltafelbediener-Warteschlange separat pollen (nur wenn aktiviert).
  const refreshSk = useCallback(() => {
    if (!manageScorekeepers) return;
    scorekeeperQueue()
      .then(setSkQueue)
      .catch(() => {});
  }, [manageScorekeepers]);

  useEffect(() => {
    if (!manageScorekeepers) {
      setSkQueue([]);
      return;
    }
    refreshSk();
    const id = window.setInterval(refreshSk, POLL_MS);
    return () => window.clearInterval(id);
  }, [manageScorekeepers, refreshSk]);

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

  // Ergebnis-Dialog öffnen: mit dem aktuellen Live-Satzstand vorbelegen (der
  // häufige Fall „Finalisieren vergessen" braucht dann nur eine Bestätigung),
  // plus eine leere Zeile zum Ergänzen. Ohne Live-Stand eine leere Startzeile.
  function openEnter(c: CourtOverview) {
    const base = c.sets.map(([a, b]) => [a, b] as [number, number]);
    setEnterSets(base.length ? [...base, [0, 0]] : [[0, 0]]);
    setError("");
    setDqConfirm(null);
    setEnterFor(c);
  }

  function setEnterCell(row: number, col: 0 | 1, value: string) {
    const n = Math.max(0, Math.min(99, Math.floor(Number(value) || 0)));
    setEnterSets((prev) =>
      prev.map((s, i) => {
        if (i !== row) return s;
        const next: [number, number] = [s[0], s[1]];
        next[col] = n;
        return next;
      }),
    );
  }

  async function submitEnterResult() {
    if (!enterFor) return;
    // 0:0-Zeilen (ungespielt / Platzhalter) verwerfen; der Server validiert
    // Satzmehrheit + Satz-Vollständigkeit zusätzlich (derive_result +
    // set_is_complete gegen das Zählformat).
    const sets = enterSets.filter(([a, b]) => a > 0 || b > 0);
    const matchId = enterFor.match_id;
    setBusy(true);
    setError("");
    try {
      await enterResult(matchId, sets);
      // Nur bei Erfolg schließen — bei einem Fehler (z. B. „Satz läuft noch")
      // bleibt der Dialog offen, damit die eingetippten Werte nicht verloren
      // gehen und die Meldung im Dialog sichtbar ist.
      setEnterFor(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  // Disqualifikation (P3): `loserTeam` wird disqualifiziert, der Gegner
  // gewinnt (BTP-ScoreStatus 3). Ein oben eingetragener Teilstand bleibt.
  async function submitDisqualify(loserTeam: 1 | 2) {
    if (!enterFor) return;
    const sets = enterSets.filter(([a, b]) => a > 0 || b > 0);
    const matchId = enterFor.match_id;
    setBusy(true);
    setError("");
    try {
      await disqualifyMatch(matchId, loserTeam, sets);
      setEnterFor(null);
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
  function allowedHallForMatch(
    m: PreparationCandidate | undefined,
  ): string | null {
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
      setError(
        "Erst oben ein Spiel wählen (oder es auf eine Feld-Spalte ziehen).",
      );
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
  const onCourtMatchIds = new Set(
    courts.map((c) => c.match_id).filter((id) => id > 0),
  );
  const assignable = candidates.filter((m) => !onCourtMatchIds.has(m.match_id));
  // Anzeige im Bestätigungs-Dialog stets aus dem Live-Stand des Felds ziehen
  // (über die stabile court_id), damit sie bei einem Poll-Wechsel nicht veraltet.
  const liveConfirm = confirmFree
    ? (courts.find((c) => c.court_id === confirmFree.court_id) ?? confirmFree)
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
      <HallFilter
        halls={allHalls}
        value={hallFilter}
        onChange={setHallFilter}
      />

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
                // Satz-/Matchball (Plan 16): nur als Planungshinweis für die
                // Turnierleitung – „Matchball" = Feld wird gleich frei. Nicht
                // bei gesperrtem Feld (dort zeigt die Karte „Gesperrt").
                const ball = occupied && !c.locked ? gamePointKind(c) : null;
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
                    className={`flex w-44 flex-col overflow-hidden rounded-xl border bg-white ${
                      ball === "match"
                        ? "border-rose-400 ring-2 ring-rose-300"
                        : ball === "set"
                          ? "border-amber-300"
                          : "border-slate-200"
                    } ${
                      clickable
                        ? "cursor-pointer hover:border-slate-400 hover:shadow-sm"
                        : ""
                    } ${blockedByHall ? "opacity-40" : ""}`}
                  >
                    {/* Spaltenkopf: Feldname + Ampelpunkt + Sperren-Schalter. */}
                    <div
                      className={`flex items-center justify-between gap-1 px-2.5 py-1.5 ${head}`}
                    >
                      <span className="flex items-center gap-1.5 font-semibold">
                        <span
                          className={`h-2 w-2 rounded-full ${
                            c.locked
                              ? "bg-rose-500"
                              : occupied
                                ? "bg-amber-500"
                                : "bg-emerald-500"
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
                          {/* Satz-/Matchball-Hinweis (Plan 16): Matchball rot +
                              pulsierend (Feld wird gleich frei), Satzball gelb. */}
                          {ball && (
                            <div
                              className={`inline-flex w-fit items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] font-bold ${
                                ball === "match"
                                  ? "animate-pulse bg-rose-200 text-rose-900"
                                  : "bg-amber-200 text-amber-900"
                              }`}
                            >
                              {ball === "match" ? "Matchball" : "Satzball"}
                            </div>
                          )}
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
                                openEnter(c);
                              }}
                              disabled={busy}
                              title="Ergebnis dieses Spiels selbst eintragen (z. B. wenn kein Tablet gezählt hat)"
                              className="rounded-md bg-emerald-200/70 px-2.5 py-1 text-xs font-medium
                                         text-emerald-900 hover:bg-emerald-200 disabled:opacity-50"
                            >
                              Ergebnis
                            </button>
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
                          {selected != null
                            ? "klicken/ziehen zum Zuweisen"
                            : "frei"}
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
                        e.dataTransfer.setData(
                          "text/plain",
                          String(m.match_id),
                        );
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
                        {spielLabel(
                          m.planned_time,
                          m.draw_name,
                          m.round_name,
                        ) || m.label}
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
                            className={
                              active ? "text-slate-300" : "text-slate-400"
                            }
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

      {/* Zähltafelbediener-Warteschlange (ADR 0007): Verlierer regulär
          beendeter Spiele, FIFO. Nur bei aktivierter Verwaltung. */}
      {manageScorekeepers && (
        <section className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-slate-700">
            Nächste Zähltafelbediener{" "}
            <span className="text-slate-400">({skQueue.length})</span>
          </h2>
          <p className="text-xs text-slate-500">
            Der Verlierer eines regulär beendeten Spiels ist als Nächster dran
            (wie im Original-BTS). Reihenfolge hier pflegen.
          </p>
          {skQueue.length > 0 && (
            <ol className="flex flex-col gap-1.5">
              {skQueue.map((e, i) => (
                <li
                  key={e.key}
                  className="flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2"
                >
                  <span className="w-5 shrink-0 text-xs font-semibold text-slate-400 tabular-nums">
                    {i + 1}.
                  </span>
                  <span className="min-w-0 flex-1 truncate text-sm">
                    {e.names.join(" / ")}
                  </span>
                  {i > 0 && (
                    <button
                      onClick={() =>
                        void advanceScorekeeper(e.key)
                          .then(refreshSk)
                          .catch((err) => setError(String(err)))
                      }
                      title="Als Nächsten dran (nach oben)"
                      className="shrink-0 rounded-md bg-slate-100 px-2 py-1 text-xs font-medium
                                 text-slate-700 hover:bg-slate-200"
                    >
                      ▲ Vorziehen
                    </button>
                  )}
                  <button
                    onClick={() =>
                      void removeScorekeeper(e.key)
                        .then(refreshSk)
                        .catch((err) => setError(String(err)))
                    }
                    title="Aus der Warteschlange entfernen"
                    className="shrink-0 rounded-md p-1 text-slate-400 hover:bg-rose-100 hover:text-rose-700"
                  >
                    <Ban size={14} />
                  </button>
                </li>
              ))}
            </ol>
          )}
          <div className="flex items-center gap-2">
            <input
              value={skAdd}
              onChange={(e) => setSkAdd(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && skAdd.trim()) {
                  void addScorekeeper([skAdd.trim()])
                    .then(() => {
                      setSkAdd("");
                      refreshSk();
                    })
                    .catch((err) => setError(String(err)));
                }
              }}
              placeholder="Manuell hinzufügen (Name)"
              className="min-w-0 flex-1 rounded-md border border-slate-300 px-2 py-1 text-sm
                         focus:border-slate-500 focus:outline-none"
            />
            <button
              onClick={() => {
                if (!skAdd.trim()) return;
                void addScorekeeper([skAdd.trim()])
                  .then(() => {
                    setSkAdd("");
                    refreshSk();
                  })
                  .catch((err) => setError(String(err)));
              }}
              disabled={!skAdd.trim()}
              className="shrink-0 rounded-md bg-slate-800 px-3 py-1 text-sm font-medium text-white
                         hover:bg-slate-700 disabled:opacity-50"
            >
              Hinzufügen
            </button>
          </div>
        </section>
      )}

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
                        {spielLabel(
                          m.planned_time,
                          m.draw_name,
                          m.round_name,
                        ) ||
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
              <h2
                id="free-confirm-title"
                className="font-semibold text-slate-800"
              >
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

      {/* Ergebnis aus der Turnierleitung eintragen (Backend-Finalisierung). */}
      {enterFor && (
        <div
          role="dialog"
          aria-modal="true"
          aria-labelledby="enter-result-title"
          className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/60 p-4"
        >
          <div className="w-full max-w-md overflow-hidden rounded-xl bg-white shadow-xl">
            <div className="border-b border-slate-200 px-5 py-3">
              <h2
                id="enter-result-title"
                className="font-semibold text-slate-800"
              >
                Ergebnis eintragen — Feld {enterFor.court}
              </h2>
            </div>
            <div className="px-5 py-4 text-sm text-slate-700">
              <p className="text-slate-600">
                {enterFor.match_name || "Spiel"} —{" "}
                {teamsLabel(enterFor.team1, enterFor.team2)}
              </p>
              <p className="mt-1 text-xs text-slate-400">
                Satzstände {enterFor.team1.join(" / ") || "Team 1"} :{" "}
                {enterFor.team2.join(" / ") || "Team 2"}. Der aktuelle Stand ist
                vorbelegt — bei Bedarf korrigieren.
              </p>
              <div className="mt-3 flex flex-col gap-2">
                {enterSets.map((s, i) => (
                  <div key={i} className="flex items-center gap-2">
                    <span className="w-12 text-xs text-slate-400">
                      Satz {i + 1}
                    </span>
                    <input
                      type="number"
                      min={0}
                      max={99}
                      inputMode="numeric"
                      value={s[0]}
                      onChange={(e) => setEnterCell(i, 0, e.target.value)}
                      className="w-16 rounded-md border border-slate-300 px-2 py-1 text-center tabular-nums"
                      aria-label={`Satz ${i + 1}, ${enterFor.team1.join(" / ") || "Team 1"}`}
                    />
                    <span className="text-slate-400">:</span>
                    <input
                      type="number"
                      min={0}
                      max={99}
                      inputMode="numeric"
                      value={s[1]}
                      onChange={(e) => setEnterCell(i, 1, e.target.value)}
                      className="w-16 rounded-md border border-slate-300 px-2 py-1 text-center tabular-nums"
                      aria-label={`Satz ${i + 1}, ${enterFor.team2.join(" / ") || "Team 2"}`}
                    />
                  </div>
                ))}
              </div>
              <button
                type="button"
                onClick={() => setEnterSets((prev) => [...prev, [0, 0]])}
                className="mt-2 text-xs font-medium text-slate-500 hover:text-slate-700"
              >
                + Satz hinzufügen
              </button>
              <p className="mt-3 text-xs text-slate-400">
                Der Sieger ergibt sich aus der Satzmehrheit; jeder Satz muss
                regulär zu Ende gespielt sein. Kampflos/Aufgabe laufen über den
                Aufgabe-Dialog am Tablet. Steht das Spiel noch auf dem Feld,
                wird es beim Eintragen freigegeben.
              </p>

              {/* Disqualifikation (P3): selten, daher dezent abgesetzt. Der
                  oben eingetippte Teilstand bleibt erhalten (Status 3). */}
              <div className="mt-3 rounded-lg border border-rose-200 bg-rose-50/60 p-2.5">
                <p className="text-xs font-semibold text-rose-800">
                  Disqualifikation
                </p>
                <p className="mt-0.5 text-[11px] text-rose-600">
                  Ein Team disqualifizieren — der Gegner gewinnt (BTP-Status 3).
                  Ein oben eingetragener Zwischenstand bleibt erhalten.
                </p>
                {dqConfirm === null ? (
                  <div className="mt-1.5 flex flex-wrap gap-2">
                    {([1, 2] as const).map((team) => {
                      const names =
                        team === 1 ? enterFor.team1 : enterFor.team2;
                      return (
                        <button
                          key={team}
                          type="button"
                          onClick={() => setDqConfirm(team)}
                          disabled={busy}
                          className="inline-flex items-center gap-1 rounded-md border border-rose-300
                                     bg-white px-2.5 py-1 text-xs font-medium text-rose-700
                                     transition-colors hover:bg-rose-100 disabled:opacity-50"
                        >
                          <Ban size={13} />
                          {(names.join(" / ") || `Team ${team}`) +
                            " disqualifizieren"}
                        </button>
                      );
                    })}
                  </div>
                ) : (
                  <div className="mt-1.5 flex flex-wrap items-center gap-2">
                    <span className="text-[11px] font-medium text-rose-800">
                      {(dqConfirm === 1 ? enterFor.team1 : enterFor.team2).join(
                        " / ",
                      ) || `Team ${dqConfirm}`}{" "}
                      wirklich disqualifizieren?
                    </span>
                    <button
                      type="button"
                      onClick={() => void submitDisqualify(dqConfirm)}
                      disabled={busy}
                      className="rounded-md bg-rose-600 px-2.5 py-1 text-xs font-semibold text-white
                                 transition-colors hover:bg-rose-700 disabled:opacity-50"
                    >
                      Ja, disqualifizieren
                    </button>
                    <button
                      type="button"
                      onClick={() => setDqConfirm(null)}
                      disabled={busy}
                      className="rounded-md bg-slate-100 px-2.5 py-1 text-xs font-medium text-slate-600
                                 transition-colors hover:bg-slate-200 disabled:opacity-50"
                    >
                      Abbrechen
                    </button>
                  </div>
                )}
              </div>
              {error && (
                <p className="mt-2 rounded-md bg-rose-50 px-3 py-2 text-sm text-rose-700">
                  {error}
                </p>
              )}
            </div>
            <div className="flex justify-end gap-2 border-t border-slate-200 bg-slate-50 px-5 py-3">
              <button
                onClick={() => setEnterFor(null)}
                disabled={busy}
                className="rounded-lg bg-slate-100 px-3.5 py-2 text-sm font-medium
                           text-slate-700 transition-colors hover:bg-slate-200 disabled:opacity-50"
              >
                Abbrechen
              </button>
              <button
                onClick={() => void submitEnterResult()}
                disabled={busy || !enterSets.some(([a, b]) => a > 0 || b > 0)}
                className="rounded-lg bg-emerald-600 px-3.5 py-2 text-sm font-medium text-white
                           transition-colors hover:bg-emerald-700 disabled:opacity-50"
              >
                Ergebnis nach BTP schreiben
              </button>
            </div>
          </div>
        </div>
      )}
    </main>
  );
}
