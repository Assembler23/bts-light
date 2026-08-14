// Eigener Menüpunkt „Schiedsrichter" (Spec docs/features/schiedsrichter-management.md):
// Rotationsreihenfolge, Pausen, Stammverein, Sperrlisten und die feldweisen
// Schalter. Die Stammliste selbst kommt aus BTP und wird hier nur gezeigt —
// gepflegt wird sie in BTP (R2).
import {
  Ban,
  Gavel,
  GripVertical,
  Info,
  ListChecks,
  Megaphone,
  Pause,
  Play,
  X,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import {
  officialAssign,
  officialClear,
  officialAppearances,
  officialBlocklists,
  officialPause,
  officialReorder,
  officialSetBlocklists,
  officialSetClub,
  officialsCourtSwitches,
  officialsRoster,
  officialsSetCourtSwitches,
  tabletOverview,
} from "../api";
import { announceOfficials } from "../io/announceCourt";
import { useDragReorder } from "../state/useDragReorder";
import type {
  AnnounceConfig,
  AppearanceView,
  AzureTtsConfig,
  CourtOverview,
  CourtSwitchesView,
  OfficialView,
  PickPlayer,
} from "../types";

/** Uhrzeit einer Endezeit (Unix-ms) — Datum spielt am Turniertag keine Rolle. */
function timeOf(ms: number | null): string {
  if (!ms) return "";
  return new Date(ms).toLocaleTimeString("de-DE", {
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function OfficialsPanel({
  enabled,
  announce,
  azureTts,
}: {
  enabled: boolean;
  announce: AnnounceConfig;
  azureTts?: AzureTtsConfig;
}) {
  const [roster, setRoster] = useState<OfficialView[]>([]);
  const [courts, setCourts] = useState<CourtSwitchesView[]>([]);
  /** Offenes Overlay: Sperrlisten-Pflege bzw. Einsatz-Liste eines Officials. */
  const [blockFor, setBlockFor] = useState<OfficialView | null>(null);
  /** Gesperrte Vereine des offenen Dialogs (als Liste, nicht als Text). */
  const [blockClubs, setBlockClubs] = useState<string[]>([]);
  /** Gesperrte Spieler des offenen Dialogs — ausgewählt, nicht getippt. */
  const [blockPlayers, setBlockPlayers] = useState<PickPlayer[]>([]);
  /** Auswahllisten des Turniers (kommen mit dem Dialog-Abruf). */
  const [pickPlayers, setPickPlayers] = useState<PickPlayer[]>([]);
  const [pickClubs, setPickClubs] = useState<string[]>([]);
  /** Suchtext der Spieler-Auswahl im Dialog. */
  const [suche, setSuche] = useState("");
  const [seenFor, setSeenFor] = useState<OfficialView | null>(null);
  const [seen, setSeen] = useState<AppearanceView[]>([]);
  const [felder, setFelder] = useState<CourtOverview[]>([]);
  /** Zuletzt gemeldeter Konflikt je Feld — die Zuweisung wird trotzdem
   *  ausgeführt, die Warnung steht nur daneben (Spec Nr. 2). */
  const [warnung, setWarnung] = useState<Record<number, string>>({});

  const laden = useCallback(() => {
    officialsRoster()
      .then(setRoster)
      .catch(() => {});
    officialsCourtSwitches()
      .then(setCourts)
      .catch(() => {});
    tabletOverview()
      .then((t) => setFelder(t.courts))
      .catch(() => {});
  }, []);

  useEffect(() => {
    laden();
    // Der Dienst-Zustand ändert sich mit jedem Feld-Aufruf — regelmäßig
    // nachladen, aber nicht so oft, dass Tippen im Vereinsfeld stört.
    const id = setInterval(laden, 4000);
    return () => clearInterval(id);
  }, [laden]);

  const oeffneSperrlisten = (o: OfficialView) => {
    setBlockFor(o);
    setSuche("");
    officialBlocklists(o.id)
      .then((b) => {
        setBlockClubs(b.clubs);
        setPickPlayers(b.pick_players);
        setPickClubs(b.pick_clubs);
        // Gespeichert sind IDs — für die Anzeige die Namen dazuholen. Wer
        // inzwischen aus der Meldeliste verschwunden ist, behält seine
        // Sperre (dann ohne Namen), statt still herauszufallen.
        setBlockPlayers(
          b.players.map(
            (id) =>
              b.pick_players.find((p) => p.id === id) ?? {
                id,
                name: `Spieler ${id}`,
                club: "",
              },
          ),
        );
      })
      .catch(() => {});
  };

  const speichereSperrlisten = () => {
    if (!blockFor) return;
    const clubs = blockClubs;
    const players = blockPlayers.map((p) => p.id);
    officialSetBlocklists(blockFor.id, clubs, players)
      .then(() => {
        setBlockFor(null);
        laden();
      })
      .catch(() => {});
  };

  const oeffneEinsaetze = (o: OfficialView) => {
    setSeenFor(o);
    officialAppearances(o.id)
      .then(setSeen)
      .catch(() => {});
  };

  const { order, registerRow, dragHandleProps, draggingId } = useDragReorder(
    roster,
    (o) => o.id,
    (id, beforeId) => {
      officialReorder(id, beforeId)
        .then(laden)
        .catch(() => {});
    },
  );

  const belegte = felder.filter((c) => c.match_id > 0);

  if (!enabled) {
    return (
      <main className="mx-auto flex min-h-full max-w-4xl flex-col gap-5 p-6 text-slate-800">
        <header>
          <h1 className="text-2xl font-semibold leading-tight">
            Schiedsrichter
          </h1>
        </header>
        <div className="flex gap-2.5 rounded-xl border border-slate-200 bg-white p-4 text-sm text-slate-500 shadow-sm">
          <Info size={18} className="mt-0.5 shrink-0 text-slate-400" />
          <span>
            Der Schiedsrichter-Betrieb ist ausgeschaltet. Er lässt sich in den
            Einstellungen unter „Schiedsrichter" einschalten — dann erscheinen
            hier die Schiedsrichter aus BTP.
          </span>
        </div>
      </main>
    );
  }

  return (
    <main className="mx-auto flex min-h-full max-w-4xl flex-col gap-5 p-6 text-slate-800">
      <header className="flex items-center gap-3">
        <div className="flex-1">
          <h1 className="text-2xl font-semibold leading-tight">
            Schiedsrichter
          </h1>
          <p className="text-sm text-slate-500">
            Reihenfolge der Rotation, Pausen und Konflikt-Angaben. Die Liste
            selbst kommt aus BTP.
          </p>
        </div>
        <span className="inline-flex items-center gap-1.5 rounded-full bg-slate-100 px-3 py-1 text-xs font-medium text-slate-600">
          <Gavel size={14} />
          {roster.length} in der Liste
        </span>
      </header>

      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-slate-700">
          Rotationsreihenfolge
        </h2>
        <p className="text-xs text-slate-500">
          Von oben nach unten wird zugeteilt. Pausierte werden übersprungen,
          behalten aber ihren Platz; nach einem Spiel rückt ein Schiedsrichter
          ans Ende. Der Verein dient der Konflikt-Warnung — BTP überträgt ihn
          nicht mit.
        </p>
        {roster.length === 0 ? (
          <div className="flex gap-2.5 rounded-xl border border-slate-200 bg-white p-4 text-sm text-slate-500 shadow-sm">
            <Info size={18} className="mt-0.5 shrink-0 text-slate-400" />
            <span>
              BTP führt für dieses Turnier keine Schiedsrichter. Sobald welche
              in BTP angelegt sind, erscheinen sie hier.
            </span>
          </div>
        ) : (
          <div className="flex flex-col gap-1.5">
            {order.map((o, i) => (
              <div
                key={o.id}
                ref={(el) => registerRow(o.id, el)}
                className={`flex flex-wrap items-center gap-2 rounded-lg border px-3 py-2 text-sm ${
                  draggingId === o.id ? "border-sky-400 bg-sky-50 shadow-md" :
                  o.paused
                    ? "border-slate-200 bg-slate-50 text-slate-400"
                    : "border-slate-200 bg-white text-slate-700"
                }`}
              >
                <span
                  {...dragHandleProps(o.id)}
                  tabIndex={0}
                  role="button"
                  title="Zum Umsortieren greifen oder mit Pfeiltasten verschieben"
                  aria-label={`${o.name} in der Reihenfolge verschieben — ziehen oder Pfeiltasten`}
                  className="cursor-grab touch-none rounded text-slate-400 outline-none focus-visible:ring-2 focus-visible:ring-sky-400 active:cursor-grabbing"
                >
                  <GripVertical size={16} />
                </span>
                <span className="w-6 text-right text-xs text-slate-400">
                  {i + 1}.
                </span>
                <span className="min-w-40 flex-1 font-medium">{o.name}</span>

                {o.on_duty_court_id != null && (
                  <span className="rounded-full bg-emerald-100 px-2 py-0.5 text-xs font-medium text-emerald-700">
                    im Dienst ({o.on_duty_role === "ar" ? "AR" : "SR"})
                  </span>
                )}
                {o.paused && (
                  <span className="rounded-full bg-slate-200 px-2 py-0.5 text-xs font-medium text-slate-600">
                    Pause
                  </span>
                )}

                <input
                  type="text"
                  defaultValue={o.club}
                  placeholder="Verein"
                  onBlur={(e) => {
                    if (e.target.value.trim() !== o.club) {
                      officialSetClub(o.id, e.target.value)
                        .then(laden)
                        .catch(() => {});
                    }
                  }}
                  className="w-40 rounded border border-slate-200 px-2 py-1 text-xs"
                />

                <button
                  type="button"
                  onClick={() => oeffneSperrlisten(o)}
                  title="Gesperrte Vereine und Spieler pflegen"
                  className="inline-flex items-center gap-1 rounded border border-slate-200 px-2 py-1 text-xs text-slate-600 hover:bg-slate-50"
                >
                  <Ban size={13} />
                  Sperren
                  {o.blocked_count > 0 && (
                    <span className="rounded-full bg-slate-200 px-1.5 text-[10px]">
                      {o.blocked_count}
                    </span>
                  )}
                </button>

                <button
                  type="button"
                  onClick={() => oeffneEinsaetze(o)}
                  title="Bisherige Einsätze anzeigen"
                  className="inline-flex items-center gap-1 rounded border border-slate-200 px-2 py-1 text-xs text-slate-600 hover:bg-slate-50"
                >
                  <ListChecks size={13} />
                  {o.appearances} Einsätze
                </button>

                <button
                  type="button"
                  onClick={() =>
                    officialPause(o.id, !o.paused)
                      .then(laden)
                      .catch(() => {})
                  }
                  title={o.paused ? "Wieder einteilen" : "Pausieren"}
                  className="inline-flex items-center gap-1 rounded border border-slate-200 px-2 py-1 text-xs text-slate-600 hover:bg-slate-50"
                >
                  {o.paused ? <Play size={13} /> : <Pause size={13} />}
                </button>
              </div>
            ))}
          </div>
        )}
      </section>

      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-slate-700">
          Einteilung der laufenden Spiele
        </h2>
        <p className="text-xs text-slate-500">
          Jederzeit änderbar, auch mitten im Spiel. Die Auswahl geht an BTP und
          erscheint dort im nächsten Abgleich; bis dahin zeigt die Zeile noch
          den Stand aus BTP.
        </p>
        {belegte.length === 0 ? (
          <div className="flex gap-2.5 rounded-xl border border-slate-200 bg-white p-4 text-sm text-slate-500 shadow-sm">
            <Info size={18} className="mt-0.5 shrink-0 text-slate-400" />
            <span>Gerade läuft kein Spiel.</span>
          </div>
        ) : (
          <div className="flex flex-col gap-1.5">
            {belegte.map((c) => (
              <div
                key={c.court_id}
                className="flex flex-wrap items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm"
              >
                <span className="min-w-24 font-medium">{c.court}</span>
                <span className="min-w-40 flex-1 truncate text-xs text-slate-500">
                  {c.match_name}
                </span>
                {(["sr", "ar"] as const).map((rolle) => (
                  <label
                    key={rolle}
                    className="flex items-center gap-1 text-xs text-slate-600"
                  >
                    {rolle.toUpperCase()}
                    <select
                      value={(rolle === "sr" ? c.sr_id : c.ar_id) || ""}
                      onChange={(e) => {
                        const wert = e.target.value;
                        const fertig = () => {
                          laden();
                        };
                        if (wert === "") {
                          setWarnung((w) => ({ ...w, [c.court_id]: "" }));
                          officialClear(c.match_id, rolle)
                            .then(fertig)
                            .catch(() => {});
                          return;
                        }
                        officialAssign(c.match_id, rolle, Number(wert))
                          .then((k) => {
                            setWarnung((w) => ({ ...w, [c.court_id]: k ?? "" }));
                            fertig();
                          })
                          .catch(() => {});
                      }}
                      className="rounded border border-slate-200 px-1.5 py-1 text-xs"
                    >
                      <option value="">—</option>
                      {roster.map((o) => (
                        <option key={o.id} value={o.id}>
                          {o.name}
                        </option>
                      ))}
                    </select>
                  </label>
                ))}
                {(warnung[c.court_id] || c.official_warn) && (
                  <span className="rounded bg-amber-200 px-1.5 py-0.5 text-xs font-medium text-amber-900">
                    Konflikt: {warnung[c.court_id] || c.official_warn}
                  </span>
                )}
                {/* Nachträgliche Zuweisungen sagen nie von selbst an
                    (Spec Nr. 8) — dafür dieser Knopf. */}
                {announce.enabled && (c.sr.length > 0 || c.ar.length > 0) && (
                  <button
                    type="button"
                    onClick={() => announceOfficials(c, announce, azureTts)}
                    title="Schiedsrichter und Aufschlagrichter ansagen"
                    className="inline-flex items-center gap-1 rounded border border-slate-200 px-2 py-1 text-xs text-slate-600 hover:bg-slate-50"
                  >
                    <Megaphone size={13} />
                    ansagen
                  </button>
                )}
              </div>
            ))}
          </div>
        )}
      </section>

      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-slate-700">Felder</h2>
        <p className="text-xs text-slate-500">
          Je Feld: ob die Rotation dort Schiedsrichter bzw. Aufschlagrichter
          einteilt und ob ein Spieler als Tabletbediener zugeteilt wird. Bedient
          der Schiedsrichter selbst, nimm „Bediener" heraus — dann verbraucht
          das Feld auch keinen Wartenden aus der Bediener-Schlange.
        </p>
        <div className="flex flex-col gap-1.5">
          {courts.map((c) => (
            <div
              key={c.court_id}
              className="flex flex-wrap items-center gap-4 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm"
            >
              <span className="min-w-32 flex-1 font-medium">{c.court}</span>
              {(
                [
                  ["Schiedsrichter", "sr"],
                  ["Aufschlagrichter", "ar"],
                  ["Bediener", "operator"],
                ] as const
              ).map(([label, key]) => (
                <label
                  key={key}
                  className="flex items-center gap-1.5 text-xs text-slate-600"
                >
                  <input
                    type="checkbox"
                    checked={c[key]}
                    onChange={(e) =>
                      officialsSetCourtSwitches(
                        c.court_id,
                        key === "sr" ? e.target.checked : c.sr,
                        key === "ar" ? e.target.checked : c.ar,
                        key === "operator" ? e.target.checked : c.operator,
                      )
                        .then(laden)
                        .catch(() => {})
                    }
                  />
                  {label}
                </label>
              ))}
            </div>
          ))}
        </div>
      </section>

      {blockFor && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 p-6">
          <div className="flex w-full max-w-lg flex-col gap-3 rounded-xl bg-white p-5 shadow-xl">
            <header className="flex items-center gap-2">
              <h3 className="flex-1 text-base font-semibold">
                Sperren für {blockFor.name}
              </h3>
              <button
                type="button"
                onClick={() => setBlockFor(null)}
                className="rounded p-1 text-slate-400 hover:bg-slate-100"
              >
                <X size={16} />
              </button>
            </header>
            <p className="text-xs text-slate-500">
              Spiele mit diesen Vereinen oder Spielern werden von der Rotation
              übersprungen; bei einer Zuweisung von Hand erscheint eine Warnung.
              Diese Angaben bleiben auf diesem Rechner und werden beim
              Turnierwechsel verworfen.
            </p>
            {/* Vereine: Auswahl aus dem Turnier, Freitext bleibt möglich —
                ein Verein, der (noch) nicht gemeldet ist, muss sich trotzdem
                sperren lassen. */}
            <div className="flex flex-col gap-1 text-xs text-slate-600">
              Gesperrte Vereine
              <div className="flex flex-wrap gap-1">
                {blockClubs.map((c) => (
                  <span
                    key={c}
                    className="inline-flex items-center gap-1 rounded-full bg-slate-100 px-2 py-0.5 text-slate-700"
                  >
                    {c}
                    <button
                      type="button"
                      onClick={() =>
                        setBlockClubs(blockClubs.filter((x) => x !== c))
                      }
                      title="Sperre entfernen"
                      className="text-slate-400 hover:text-slate-700"
                    >
                      <X size={11} />
                    </button>
                  </span>
                ))}
                {blockClubs.length === 0 && (
                  <span className="text-slate-400">keine</span>
                )}
              </div>
              <input
                type="text"
                list="pick-clubs"
                placeholder="Verein wählen oder eingeben, dann Enter"
                onKeyDown={(e) => {
                  if (e.key !== "Enter") return;
                  e.preventDefault();
                  const wert = e.currentTarget.value.trim();
                  if (wert && !blockClubs.includes(wert)) {
                    setBlockClubs([...blockClubs, wert]);
                  }
                  e.currentTarget.value = "";
                }}
                className="rounded border border-slate-200 px-2 py-1 text-sm"
              />
              <datalist id="pick-clubs">
                {pickClubs.map((c) => (
                  <option key={c} value={c} />
                ))}
              </datalist>
            </div>

            {/* Spieler: ausschließlich Auswahl. Eine BTP-Spieler-ID kennt
                niemand auswendig — getippt wird der Name, gespeichert die ID. */}
            <div className="flex flex-col gap-1 text-xs text-slate-600">
              Gesperrte Spieler
              <div className="flex flex-wrap gap-1">
                {blockPlayers.map((p) => (
                  <span
                    key={p.id}
                    className="inline-flex items-center gap-1 rounded-full bg-slate-100 px-2 py-0.5 text-slate-700"
                  >
                    {p.name}
                    {p.club && (
                      <span className="text-slate-400">({p.club})</span>
                    )}
                    <button
                      type="button"
                      onClick={() =>
                        setBlockPlayers(
                          blockPlayers.filter((x) => x.id !== p.id),
                        )
                      }
                      title="Sperre entfernen"
                      className="text-slate-400 hover:text-slate-700"
                    >
                      <X size={11} />
                    </button>
                  </span>
                ))}
                {blockPlayers.length === 0 && (
                  <span className="text-slate-400">keine</span>
                )}
              </div>
              <input
                type="text"
                value={suche}
                onChange={(e) => setSuche(e.target.value)}
                placeholder="Spieler suchen …"
                className="rounded border border-slate-200 px-2 py-1 text-sm"
              />
              {suche.trim().length >= 2 && (
                <ul className="max-h-40 overflow-y-auto rounded border border-slate-200">
                  {pickPlayers
                    .filter(
                      (p) =>
                        !blockPlayers.some((x) => x.id === p.id) &&
                        `${p.name} ${p.club}`
                          .toLowerCase()
                          .includes(suche.trim().toLowerCase()),
                    )
                    .slice(0, 25)
                    .map((p) => (
                      <li key={p.id}>
                        <button
                          type="button"
                          onClick={() => {
                            setBlockPlayers([...blockPlayers, p]);
                            setSuche("");
                          }}
                          className="flex w-full items-center gap-2 px-2 py-1 text-left text-sm hover:bg-slate-50"
                        >
                          <span className="flex-1">{p.name}</span>
                          <span className="text-xs text-slate-400">
                            {p.club}
                          </span>
                        </button>
                      </li>
                    ))}
                </ul>
              )}
            </div>

            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setBlockFor(null)}
                className="rounded border border-slate-200 px-3 py-1.5 text-sm text-slate-600 hover:bg-slate-50"
              >
                Abbrechen
              </button>
              <button
                type="button"
                onClick={speichereSperrlisten}
                className="rounded bg-slate-800 px-3 py-1.5 text-sm font-medium text-white hover:bg-slate-700"
              >
                Speichern
              </button>
            </div>
          </div>
        </div>
      )}

      {seenFor && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 p-6">
          <div className="flex w-full max-w-lg flex-col gap-3 rounded-xl bg-white p-5 shadow-xl">
            <header className="flex items-center gap-2">
              <h3 className="flex-1 text-base font-semibold">
                Einsätze von {seenFor.name}
              </h3>
              <button
                type="button"
                onClick={() => setSeenFor(null)}
                className="rounded p-1 text-slate-400 hover:bg-slate-100"
              >
                <X size={16} />
              </button>
            </header>
            {seen.length === 0 ? (
              <p className="text-sm text-slate-500">
                Noch kein beendetes Spiel.
              </p>
            ) : (
              <ul className="flex flex-col gap-1 text-sm">
                {seen.map((a) => (
                  <li
                    key={`${a.match_id}-${a.role}`}
                    className="flex items-center gap-3 rounded border border-slate-200 px-2 py-1.5"
                  >
                    <span className="w-8 text-xs font-medium text-slate-500">
                      {a.role === "ar" ? "AR" : "SR"}
                    </span>
                    <span className="flex-1">{a.match_name}</span>
                    <span className="text-xs text-slate-500">{a.court}</span>
                    <span className="text-xs text-slate-400">
                      {timeOf(a.finished_at)}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      )}
    </main>
  );
}
