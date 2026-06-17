import { useEffect, useMemo, useState } from "react";
import { Megaphone, Volume2, X } from "lucide-react";
import { callPreparation, preparationCandidates, retractPreparation } from "../api";
import {
  playPreparationAnnouncement,
  resolveAnnouncementLanguage,
} from "../io/announcer";
import { azureOption } from "../io/azureAnnounce";
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

  const hallName = multiHall
    ? locations.find((l) => l.id === hallId)?.name ?? ""
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
          <ul className="flex flex-col gap-1.5">
            {open.map((c) => (
              <li key={c.match_id}>
                <label className="flex cursor-pointer items-center gap-2.5 rounded-lg border border-slate-200 px-3 py-2 transition-colors hover:bg-slate-50">
                  <input
                    type="checkbox"
                    checked={checked.has(c.match_id)}
                    onChange={() => toggle(c.match_id)}
                    className="size-4 accent-sky-600"
                  />
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className="text-sm">
                      <span className="font-medium">
                        {c.label || "Spiel"}
                      </span>
                      {c.match_num !== null && (
                        <span className="text-slate-400">
                          {" "}
                          · Nr. {c.match_num}
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
                      <span className="text-slate-500">
                        {" "}
                        · {c.call.hall}
                      </span>
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
