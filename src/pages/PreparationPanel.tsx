import { useEffect, useMemo, useState } from "react";
import { GripVertical, Megaphone, RotateCcw, Volume2, X } from "lucide-react";
import {
  callPreparation,
  preparationCandidates,
  queueOrderReset,
  queueReorder,
  retractPreparation,
} from "../api";
import {
  playPreparationAnnouncement,
  resolveAnnouncementLanguage,
} from "../io/announcer";
import { azureOption } from "../io/azureAnnounce";
import { useDragReorder } from "../state/useDragReorder";
import type {
  AnnounceConfig,
  AzureTtsConfig,
  Discipline,
  PreparationCandidate,
  PreparationLocation,
} from "../types";

interface Props {
  /** Ansage-Einstellungen aus der App-Konfiguration. Bei `enabled=false`
   *  wird kein „Ansage"-Knopf gezeigt. */
  announce: AnnounceConfig;
  azureTts?: AzureTtsConfig;
}

/**
 * Tab „In Vorbereitung" der Tablet-Seite. Die Turnierleitung wählt
 * eingeplante Spiele aus und „ruft sie in die Vorbereitung" – optional je
 * Halle. Der Aufruf bekommt einen Zeitstempel, der im Liveticker-Payload
 * mitgeht; der `display=next`-Monitor hebt gerufene Spiele dann hervor
 * („In Vorbereitung · seit X Min"). BTP kennt keinen Vorbereitungs-Zustand
 * – bts-light verwaltet ihn selbst. Pollt die Kandidaten alle 4 s.
 *
 * Je gerufenem Spiel gibt es einen „Ansage"-Knopf, der eine gesprochene
 * Hallen-Ansage auslöst (sofern Ansagen aktiviert sind) – analog zur
 * Feld-Ansage beim Court-Aufruf, aber ohne Feld, dafür mit Halle.
 */
export function PreparationPanel({ announce, azureTts }: Props) {
  const [candidates, setCandidates] = useState<PreparationCandidate[]>([]);
  const [locations, setLocations] = useState<PreparationLocation[]>([]);
  const [checked, setChecked] = useState<Set<number>>(new Set());
  // Gewählte Halle für den Aufruf (LocationID); null = ohne Halle.
  const [hallId, setHallId] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  // Zweitaufruf je Partei (Plan 1): Schlüssel `${match_id}:a|b` → zuletzt
  // gesagte Stufe. Erster Nachruf = 2 („Zweiter Aufruf"), weitere = 3
  // („Dritter und letzter Aufruf"). Nur im Master-Fenster, kein Server-State.
  const [callStages, setCallStages] = useState<Map<string, 2 | 3>>(new Map());

  useEffect(() => {
    let alive = true;
    const tick = () => {
      preparationCandidates()
        .then((v) => {
          if (!alive) return;
          setCandidates(v.candidates);
          setLocations(v.locations);
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

  // Erst ab zwei Hallen ist die Hallen-Auswahl sinnvoll (Mehr-Hallen-
  // Turnier). Bei einem Ein-Hallen-Turnier wird ohne Halle gerufen.
  const multiHall = locations.length >= 2;

  // Zeigt der Spielplan die gewählte Halle nach einer BTP-Topologie-
  // Änderung nicht mehr, die Auswahl verwerfen – die Vorauswahl unten
  // greift dann neu.
  useEffect(() => {
    if (hallId !== null && !locations.some((l) => l.id === hallId)) {
      setHallId(null);
    }
  }, [locations, hallId]);

  // Bei einem Mehr-Hallen-Turnier eine sinnvolle Vorauswahl treffen.
  useEffect(() => {
    if (multiHall && hallId === null && locations.length > 0) {
      setHallId(locations[0].id);
    }
  }, [multiHall, hallId, locations]);

  // Noch nicht gerufene Kandidaten (auswählbar) und bereits gerufene.
  const open = useMemo(
    () => candidates.filter((c) => c.call === null),
    [candidates],
  );
  const called = useMemo(
    () => candidates.filter((c) => c.call !== null),
    [candidates],
  );

  // Offene Kandidaten je Halle gruppiert (Spec
  // `spielliste-manuelle-reihenfolge`): ein Zug darf Match-IDs nur innerhalb
  // DERSELBEN Halle relativ zueinander verschieben, sonst löste
  // `assign::hall_for_match` serverseitig ein stilles No-Op aus (ADR 0023).
  // Bei nur einer Halle bleibt es bei einer einzigen, unbeschrifteten Gruppe.
  const openGroups = useMemo((): [string, PreparationCandidate[]][] => {
    if (!multiHall) return [["", open]];
    const byHall = new Map<string, PreparationCandidate[]>();
    for (const c of open) {
      const key = c.hall || "";
      if (!byHall.has(key)) byHall.set(key, []);
      byHall.get(key)!.push(c);
    }
    return [...byHall.entries()].sort(([a], [b]) => a.localeCompare(b, "de"));
  }, [open, multiHall]);

  // Auswahl auf noch offene Kandidaten beschränken (gerufene rausfiltern).
  useEffect(() => {
    setChecked((prev) => {
      const openIds = new Set(open.map((c) => c.match_id));
      const next = new Set([...prev].filter((id) => openIds.has(id)));
      return next.size === prev.size ? prev : next;
    });
  }, [open]);

  const toggle = (id: number) => {
    setChecked((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const refresh = () =>
    preparationCandidates()
      .then((v) => {
        setCandidates(v.candidates);
        setLocations(v.locations);
      })
      .catch(() => {});

  // Ein noch nicht gerufenes Spiel vor ein anderes ziehen (Spec
  // `spielliste-manuelle-reihenfolge`) — die Halle wird serverseitig aus
  // dem Match abgeleitet, hier wird nur (id, beforeId) übertragen.
  const reorderOpen = (matchId: number, beforeMatchId: number | null) => {
    queueReorder(matchId, beforeMatchId)
      .then(refresh)
      .catch(() => {});
  };

  const resetQueueOrder = () => {
    queueOrderReset()
      .then(refresh)
      .catch(() => {});
  };

  const callSelected = async () => {
    if (checked.size === 0) return;
    setBusy(true);
    try {
      await callPreparation([...checked], multiHall ? hallId : null);
      setChecked(new Set());
      await refresh();
    } catch {
      /* Fehler ignorieren – der nächste Poll korrigiert die Anzeige */
    } finally {
      setBusy(false);
    }
  };

  const retract = async (matchId: number) => {
    setBusy(true);
    // Nachruf-Zähler dieses Spiels vergessen: wird es später erneut gerufen,
    // beginnt der Nachruf wieder bei „Zweiter Aufruf" (statt am alten Stand
    // hängen zu bleiben). Räumt zugleich die Map auf.
    setCallStages((m) => {
      const n = new Map(m);
      n.delete(`${matchId}:a`);
      n.delete(`${matchId}:b`);
      return n;
    });
    try {
      await retractPreparation(matchId);
      await refresh();
    } catch {
      /* ignorieren */
    } finally {
      setBusy(false);
    }
  };

  // Spielt die Vorbereitungs-Ansage für ein gerufenes Spiel: Halle aus dem
  // Aufruf, Sprache automatisch oder per Konfiguration. Der Knopf-Klick
  // selbst ist die User-Geste, mit der WebView2 den AudioContext entsperrt
  // — ein separater unlockAudio()-Aufruf ist hier nicht nötig.
  const announceCandidate = (c: PreparationCandidate) => {
    const lang = resolveAnnouncementLanguage(
      [...c.team1_nationalities, ...c.team2_nationalities],
      announce.language_mode,
    );
    void playPreparationAnnouncement(
      {
        discipline: (c.discipline || "unknown") as Discipline,
        className: c.class_label,
        teamANames: c.team1,
        teamBNames: c.team2,
        hall: c.call?.hall || undefined,
      },
      lang,
      {
        rate: announce.rate,
        voiceURI: lang === "de" ? announce.voice_de : announce.voice_en,
        gong: announce.gong,
        nameOverrides: announce.name_overrides,
        nameOverridesEnabled: announce.name_overrides_enabled,
        azure: azureOption(azureTts),
      },
    );
  };

  // Gezielter Zweit-/Drittaufruf NUR einer Partei (die noch fehlt) — nennt
  // wie bei Tilo nur diese eine Seite. Erster Nachruf = „Zweiter Aufruf",
  // jeder weitere = „Dritter und letzter Aufruf".
  const secondCall = (c: PreparationCandidate, side: "a" | "b") => {
    const key = `${c.match_id}:${side}`;
    const stage: 2 | 3 = callStages.get(key) ? 3 : 2;
    setCallStages((m) => new Map(m).set(key, stage));
    const names = side === "a" ? c.team1 : c.team2;
    const nats = side === "a" ? c.team1_nationalities : c.team2_nationalities;
    const lang = resolveAnnouncementLanguage(nats, announce.language_mode);
    void playPreparationAnnouncement(
      {
        discipline: (c.discipline || "unknown") as Discipline,
        className: c.class_label,
        teamANames: names,
        teamBNames: [], // nur die fehlende Partei ansagen
        hall: c.call?.hall || undefined,
        callStage: stage,
      },
      lang,
      {
        rate: announce.rate,
        voiceURI: lang === "de" ? announce.voice_de : announce.voice_en,
        gong: announce.gong,
        nameOverrides: announce.name_overrides,
        nameOverridesEnabled: announce.name_overrides_enabled,
        azure: azureOption(azureTts),
      },
    );
  };

  const hallName = multiHall
    ? (locations.find((l) => l.id === hallId)?.name ?? "")
    : "";
  const callLabel = busy
    ? "Wird aufgerufen …"
    : multiHall && hallName
      ? `In ${hallName} aufrufen`
      : "Aufrufen";

  return (
    <section className="flex flex-col gap-3">
      <p className="text-xs text-slate-500">
        Eingeplante Spiele „in die Vorbereitung" rufen – sie werden auf der
        Aufruf-Anzeige (display=next) hervorgehoben. BTP kennt keinen
        Vorbereitungs-Zustand; bts-light verwaltet ihn selbst.
      </p>

      {/* Offene Kandidaten mit Auswahl-Checkboxen. */}
      {open.length === 0 ? (
        <p className="rounded-xl border border-slate-200 bg-white p-4 text-sm text-slate-500 shadow-sm">
          Keine eingeplanten Spiele zum Aufrufen. Sobald Paarungen feststehen
          und noch nicht auf einem Feld laufen, erscheinen sie hier.
        </p>
      ) : (
        <div className="flex flex-col gap-2 rounded-xl border border-slate-200 bg-white p-4 shadow-sm">
          {/* Reset-Knopf: verwirft die manuelle Reihenfolge ALLER Hallen auf
              einmal (Spec `spielliste-manuelle-reihenfolge`) — nur sichtbar,
              solange irgendein Spiel manuell einsortiert ist. */}
          {candidates.some((c) => c.manual) && (
            <div className="flex justify-end">
              <button
                onClick={resetQueueOrder}
                title="Verwirft die manuelle Sortierung ALLER Hallen — danach gilt wieder BTPs eigene Reihenfolge"
                className="inline-flex items-center gap-1 rounded-md px-1.5 py-1 text-xs
                           font-medium text-slate-500 transition-colors hover:bg-slate-100
                           hover:text-slate-700"
              >
                <RotateCcw size={13} />
                Reihenfolge zurücksetzen
              </button>
            </div>
          )}
          {openGroups.map(([hall, items]) => (
            <OpenHallGroup
              key={hall}
              hall={hall}
              items={items}
              showHeading={openGroups.length > 1}
              checked={checked}
              toggle={toggle}
              onReorder={reorderOpen}
            />
          ))}

          {/* Aufruf-Zeile: Hallen-Auswahl (nur Mehr-Hallen) + Button. */}
          <div className="mt-1 flex items-center justify-end gap-2">
            {multiHall && (
              <select
                value={hallId ?? ""}
                onChange={(e) => setHallId(Number(e.target.value))}
                className="rounded-lg border border-slate-300 bg-white px-2.5 py-1.5
                           text-sm text-slate-700"
              >
                {locations.map((l) => (
                  <option key={l.id} value={l.id}>
                    {l.name}
                  </option>
                ))}
              </select>
            )}
            <button
              onClick={callSelected}
              disabled={busy || checked.size === 0}
              className="inline-flex items-center gap-1.5 rounded-lg bg-sky-600 px-3 py-1.5
                         text-sm font-medium text-white transition-colors
                         hover:bg-sky-700 disabled:opacity-50"
            >
              <Megaphone size={15} />
              {callLabel}
            </button>
          </div>
        </div>
      )}

      {/* Bereits gerufene Spiele. */}
      {called.length > 0 && (
        <div className="flex flex-col gap-2">
          <h3 className="mt-1 text-sm font-semibold text-slate-600">
            In Vorbereitung
          </h3>
          <ul className="flex flex-col gap-1.5">
            {called.map((c) => (
              <li
                key={c.match_id}
                className="flex items-center gap-3 rounded-lg border border-sky-200
                           bg-sky-50 px-3 py-2"
              >
                <Megaphone size={16} className="shrink-0 text-sky-600" />
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="text-sm">
                    <span className="font-medium">{c.label || "Spiel"}</span>
                    {c.call?.hall && (
                      <span className="text-slate-500"> · {c.call.hall}</span>
                    )}
                  </span>
                  <span className="truncate text-xs text-slate-500">
                    {c.team1.length > 0 ? c.team1.join(" / ") : "—"}{" "}
                    <span className="text-slate-400">gegen</span>{" "}
                    {c.team2.length > 0 ? c.team2.join(" / ") : "—"}
                  </span>
                </span>
                {c.call && (
                  <span className="shrink-0 text-xs text-sky-700">
                    {sinceLabel(c.call.called_at_ms)}
                  </span>
                )}
                {announce.enabled && (
                  <button
                    onClick={() => announceCandidate(c)}
                    disabled={busy}
                    title="Hallen-Ansage abspielen"
                    className="inline-flex shrink-0 items-center gap-1 rounded-md px-1.5
                               py-1 text-xs font-medium text-sky-700 transition-colors
                               hover:bg-sky-100 disabled:opacity-50"
                  >
                    <Volume2 size={14} />
                    Ansage
                  </button>
                )}
                {/* Gezielter Zweit-/Drittaufruf je Partei: nennt nur die
                    genannte Seite (die noch fehlt). */}
                {announce.enabled &&
                  (["a", "b"] as const).map((side) => {
                    const team = side === "a" ? c.team1 : c.team2;
                    if (team.length === 0) return null;
                    const nextStage = callStages.get(`${c.match_id}:${side}`)
                      ? 3
                      : 2;
                    const shortName =
                      team[0].trim().split(" ").filter(Boolean).slice(-1)[0] ||
                      team[0];
                    return (
                      <button
                        key={side}
                        onClick={() => secondCall(c, side)}
                        disabled={busy}
                        title={`${nextStage === 3 ? "Dritter und letzter" : "Zweiter"} Aufruf für ${team.join(" / ")}`}
                        className="inline-flex shrink-0 items-center gap-1 rounded-md
                                   bg-amber-100 px-1.5 py-1 text-xs font-medium text-amber-800
                                   transition-colors hover:bg-amber-200 disabled:opacity-50"
                      >
                        {side === "a" ? "◂" : "▸"} {nextStage}. Ruf {shortName}
                      </button>
                    );
                  })}
                <button
                  onClick={() => retract(c.match_id)}
                  disabled={busy}
                  title="Aufruf zurücknehmen"
                  className="inline-flex shrink-0 items-center gap-1 rounded-md px-1.5
                             py-1 text-xs font-medium text-slate-500 transition-colors
                             hover:bg-slate-200 hover:text-slate-700 disabled:opacity-50"
                >
                  <X size={14} />
                  Aufruf zurücknehmen
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}

/** „vor X Min." (bzw. „gerade eben") seit dem Aufruf-Zeitstempel. */
function sinceLabel(calledAtMs: number): string {
  const mins = Math.floor((Date.now() - calledAtMs) / 60000);
  if (mins <= 0) return "gerade eben";
  return `vor ${mins} Min.`;
}

/**
 * Eine Hallen-Gruppe der offenen Kandidaten, ziehbar per `useDragReorder`
 * (Spec `spielliste-manuelle-reihenfolge`, gleiche Bausteine wie
 * `OfficialsPanel`). Eigene Komponente, weil `useDragReorder` je Hallen-
 * Gruppe genau einmal aufgerufen werden muss (Hook-Regeln — eine dynamische
 * Anzahl Hallen verbietet den Hook-Aufruf direkt in einer Schleife).
 */
function OpenHallGroup({
  hall,
  items,
  showHeading,
  checked,
  toggle,
  onReorder,
}: {
  hall: string;
  items: PreparationCandidate[];
  showHeading: boolean;
  checked: Set<number>;
  toggle: (id: number) => void;
  onReorder: (matchId: number, beforeMatchId: number | null) => void;
}) {
  const { order, registerRow, dragHandleProps } = useDragReorder(
    items,
    (c) => c.match_id,
    onReorder,
  );
  return (
    <div className="flex flex-col gap-1.5">
      {showHeading && (
        <h4 className="mt-1 text-xs font-semibold uppercase tracking-wide text-slate-400">
          {hall || "Ohne Hallenzuordnung"}
        </h4>
      )}
      <ul className="flex flex-col gap-1.5">
        {order.map((c) => (
          <li
            key={c.match_id}
            ref={(el) => registerRow(c.match_id, el)}
            className="flex items-center gap-1.5 rounded-lg border border-slate-200
                       px-1.5 py-1 transition-colors hover:bg-slate-50"
          >
            <span
              {...dragHandleProps(c.match_id)}
              tabIndex={0}
              role="button"
              title="Zum Umsortieren greifen oder mit Pfeiltasten verschieben"
              aria-label={`${c.label || "Spiel"} in der Reihenfolge verschieben — ziehen oder Pfeiltasten`}
              className="cursor-grab touch-none rounded text-slate-400 outline-none
                         focus-visible:ring-2 focus-visible:ring-sky-400 active:cursor-grabbing"
            >
              <GripVertical size={16} />
            </span>
            <label className="flex min-w-0 flex-1 cursor-pointer items-center gap-2.5 py-1">
              <input
                type="checkbox"
                checked={checked.has(c.match_id)}
                onChange={() => toggle(c.match_id)}
                className="size-4 accent-sky-600"
              />
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="text-sm">
                  <span className="font-medium">{c.label || "Spiel"}</span>
                  {c.match_num !== null && (
                    <span className="text-slate-400"> · Nr. {c.match_num}</span>
                  )}
                  {c.manual && (
                    <span className="ml-1.5 text-xs font-semibold text-sky-600">
                      Manuell einsortiert
                    </span>
                  )}
                </span>
                <span className="truncate text-xs text-slate-500">
                  {c.team1.length > 0 ? c.team1.join(" / ") : "—"}{" "}
                  <span className="text-slate-400">gegen</span>{" "}
                  {c.team2.length > 0 ? c.team2.join(" / ") : "—"}
                </span>
              </span>
            </label>
          </li>
        ))}
      </ul>
    </div>
  );
}
