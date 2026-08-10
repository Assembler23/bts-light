import { useEffect, useMemo, useState } from "react";
import {
  CheckCircle2,
  Clock,
  Lock,
  Megaphone,
  RotateCcw,
  Unlock,
} from "lucide-react";
import {
  checkinAnnouncement,
  checkinSetPlayer,
  checkinSetTimes,
  checkinState,
  publishFreetext,
} from "../api";
import type {
  AnnounceConfig,
  CheckinClass,
  CheckinPlayer,
  CheckinView,
} from "../types";

/** Poll-Takt. Bewusst träger als die 4 s der Vorbereitungs-Seite: badhub läuft
 *  auf Shared Hosting und bedient zur Fensteröffnung gleichzeitig die halbe
 *  Halle. Der Check-In ändert sich in Minuten, nicht in Sekunden. */
const POLL_MS = 15000;

/** Zustands-Text einer Klasse. Die Zustände kommen fertig aus badhub — sie
 *  werden dort in Europe/Berlin berechnet (B2), damit eine falsch gestellte
 *  Uhr auf diesem Rechner nichts verschiebt. */
function klassenZustand(k: CheckinClass): { text: string; klasse: string } {
  switch (k.state) {
    case "open":
      return { text: "Check-In läuft", klasse: "bg-emerald-100 text-emerald-800" };
    case "pending":
      return {
        text: k.opens_at ? `öffnet ${uhrzeit(k.opens_at)}` : "öffnet später",
        klasse: "bg-slate-100 text-slate-600",
      };
    case "closed":
      return { text: "Anmeldeschluss vorbei", klasse: "bg-amber-100 text-amber-800" };
    case "live":
      return { text: "läuft bereits", klasse: "bg-violet-100 text-violet-800" };
    default:
      // unscheduled: die Turnierleitung hat noch keine Anfangszeit gepflegt.
      // Das ist der einzige Zustand, den sie selbst auflösen kann — deshalb
      // auffällig und nicht grau wie „öffnet später".
      return { text: "keine Anfangszeit", klasse: "bg-rose-100 text-rose-800" };
  }
}

/** „2026-08-15 09:00:00" → „09:00". */
function uhrzeit(wert: string): string {
  return wert.slice(11, 16);
}

/** DB-Form („2026-08-15 09:00:00") → Wert für `<input type="datetime-local">`. */
function fuerEingabe(wert: string | null): string {
  return wert ? wert.slice(0, 16).replace(" ", "T") : "";
}

/** Zustand eines Spielers in Worten. */
function spielerZustand(p: CheckinPlayer): { text: string; klasse: string } {
  if (p.state === "checked_in") {
    const woher =
      p.source === "official"
        ? " (Turnierleitung)"
        : p.source === "partner"
          ? " (durch Partner)"
          : "";
    return { text: `da${woher}`, klasse: "text-emerald-700" };
  }
  if (p.state === "query") {
    return { text: "Rückfrage an Turnierleitung", klasse: "text-amber-700" };
  }
  if (p.state === "withdrawn") {
    // In badhub abgemeldet: weder da noch gesucht. Durchgestrichen, damit
    // der Blick beim Durchgehen der Liste nicht an der Zeile hängen bleibt.
    return { text: "abgemeldet", klasse: "text-slate-400 line-through" };
  }
  if (p.locked) {
    return { text: "zurückgesetzt, gesperrt", klasse: "text-rose-700" };
  }
  return { text: "fehlt", klasse: "text-slate-500" };
}

/**
 * Seite „Check-In" — die Sicht der Turnierleitung auf den Hallen-Check-In
 * (Spezifikation Schnitt C, AK-C1 bis C15).
 *
 * Sie zeigt je Klasse, wer da ist und wer fehlt, lässt Spieler von Hand setzen
 * und zurücksetzen und die Zeiten am Turniertag ändern.
 *
 * **Alles läuft über Tauri-Commands** (Architekturregel R1), nie per `fetch()`
 * gegen badhub: das Liveticker-Passwort bleibt damit im Backend.
 *
 * **Kein eigener Zwischenspeicher.** badhub speichert, diese Seite zeigt an
 * (AK-C13). Nach jeder Änderung wird neu abgerufen, statt den Stand lokal
 * fortzuschreiben — sonst stünde nach einem abgelehnten Schreibvorgang etwas
 * anderes auf dem Bildschirm als in der Datenbank.
 */
export function CheckinPanel({ announce }: { announce: AnnounceConfig }) {
  const [view, setView] = useState<CheckinView | null>(null);
  const [offen, setOffen] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);
  const [fehler, setFehler] = useState<string>("");
  /** Letzte Ansage als Rückmeldung — gesprochen wird woanders, hier soll
   *  sichtbar sein, dass der Klick angekommen ist. */
  const [gesagt, setGesagt] = useState<string>("");

  const laden = () =>
    checkinState()
      .then(setView)
      // `checkin_state` liefert nie Err — der Catch ist der Gürtel zum
      // Hosenträger, damit ein unerwarteter Fehler die Seite nicht leer lässt.
      .catch(() => {});

  useEffect(() => {
    let alive = true;
    const tick = () => {
      checkinState()
        .then((v) => {
          if (alive) setView(v);
        })
        .catch(() => {});
    };
    tick();
    const id = setInterval(tick, POLL_MS);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);

  const klassen = view?.classes ?? [];
  const gesamt = useMemo(
    () =>
      klassen.reduce(
        (acc, k) => {
          const abgemeldet = k.players.filter(
            (p) => p.state === "withdrawn",
          ).length;
          return {
            gemeldet: acc.gemeldet + k.gemeldet - abgemeldet,
            eingecheckt: acc.eingecheckt + k.eingecheckt,
            abgemeldet: acc.abgemeldet + abgemeldet,
          };
        },
        { gemeldet: 0, eingecheckt: 0, abgemeldet: 0 },
      ),
    [klassen],
  );

  async function eingriff(
    eventId: number,
    playerId: number,
    action: "check_in" | "reset" | "unlock",
  ) {
    setBusy(true);
    setFehler("");
    try {
      await checkinSetPlayer(eventId, playerId, action);
      await laden();
    } catch (e) {
      setFehler(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function zeiten(
    eventId: number,
    startsAt: string,
    closesAt: string,
  ) {
    setBusy(true);
    setFehler("");
    try {
      // Leeres Eingabefeld heißt „Wert löschen", nicht „leerer String".
      await checkinSetTimes(eventId, startsAt || null, closesAt || null);
      await laden();
    } catch (e) {
      setFehler(String(e));
    } finally {
      setBusy(false);
    }
  }

  /** Ansage bauen lassen und in die Halle geben (AK-C6, C7, C10).
   *
   *  Der Text entsteht im Backend aus einem **frisch geholten** Stand — nicht
   *  aus dem, was gerade auf dem Bildschirm steht. Gerufen wird ohne
   *  Hallen-Filter (AK-C9): eine Klasse startet zwar in einer Halle, der
   *  Check-In gilt aber turnierweit. */
  async function ansagen(eventId: number, kind: "deadline" | "missing") {
    setBusy(true);
    setFehler("");
    setGesagt("");
    try {
      const text = await checkinAnnouncement(eventId, kind);
      if (!text) {
        setGesagt("Es gibt gerade nichts anzusagen.");
        return;
      }
      await publishFreetext("", text);
      setGesagt(text);
    } catch (e) {
      setFehler(String(e));
    } finally {
      setBusy(false);
    }
  }

  // ── Zustände ohne Inhalt ──────────────────────────────────────────────
  if (!view) {
    return <p className="p-4 text-sm text-slate-500">Check-In wird geladen …</p>;
  }

  if (view.availability !== "ready") {
    // AK-C3/C4: kein Fehlerbild, sondern ein Satz, der erklärt, was los ist.
    // Das übrige Programm bleibt uneingeschränkt bedienbar.
    return (
      <div className="p-4">
        <h2 className="text-base font-semibold text-slate-800">Check-In</h2>
        <p className="mt-2 max-w-prose text-sm text-slate-600">
          {view.message ||
            "Der Check-In braucht Internet — badhub ist gerade nicht erreichbar."}
        </p>
        {view.availability === "offline" && (
          <p className="mt-2 max-w-prose text-xs text-slate-500">
            Das Turnier läuft unverändert weiter: der Check-In hängt weder an
            der Feldvergabe noch an den Ergebnissen.
          </p>
        )}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3 p-4">
      <div className="flex items-baseline justify-between">
        <h2 className="text-base font-semibold text-slate-800">
          Check-In{view.tournament_name ? ` — ${view.tournament_name}` : ""}
        </h2>
        {gesamt.gemeldet > 0 && (
          <span className="text-sm text-slate-500">
            {gesamt.eingecheckt} von {gesamt.gemeldet} da
            {gesamt.abgemeldet > 0 ? ` · ${gesamt.abgemeldet} abgemeldet` : ""}
          </span>
        )}
      </div>

      {fehler && (
        <p className="rounded border border-rose-200 bg-rose-50 px-3 py-2 text-sm text-rose-800">
          {fehler}
        </p>
      )}

      {gesagt && (
        <p className="rounded border border-slate-200 bg-slate-50 px-3 py-2 text-sm text-slate-700">
          {gesagt}
        </p>
      )}

      {klassen.length === 0 && (
        <p className="max-w-prose text-sm text-slate-600">
          {view.message ||
            "Für dieses Turnier liegt in badhub noch keine Meldeliste. Sie wird " +
              "automatisch aus BTP übertragen, sobald Klassen und Meldungen dort stehen."}
        </p>
      )}

      {klassen.map((k) => {
        const zustand = klassenZustand(k);
        const aufgeklappt = offen.has(k.event_id);
        // badhubs TL-Zaehlung schliesst Abgemeldete bewusst ein (dortige
        // Entscheidung) — hier werden sie herausgerechnet, denn fuer die
        // Turnierleitung sind sie weder da noch fehlend.
        const abgemeldet = k.players.filter(
          (p) => p.state === "withdrawn",
        ).length;
        const gemeldet = k.gemeldet - abgemeldet;
        const fehlend = gemeldet - k.eingecheckt;
        return (
          <section
            key={k.event_id}
            className="rounded border border-slate-200 bg-white"
          >
            <header className="flex flex-wrap items-center gap-3 px-3 py-2">
              <button
                onClick={() =>
                  setOffen((prev) => {
                    const next = new Set(prev);
                    if (next.has(k.event_id)) next.delete(k.event_id);
                    else next.add(k.event_id);
                    return next;
                  })
                }
                className="text-left text-sm font-medium text-slate-800 hover:underline"
              >
                {k.name || `Klasse ${k.event_id}`}
              </button>
              <span
                className={`rounded px-2 py-0.5 text-xs font-medium ${zustand.klasse}`}
              >
                {zustand.text}
              </span>
              <span className="text-xs text-slate-500">
                {k.eingecheckt} von {gemeldet} da
                {fehlend > 0 ? ` · ${fehlend} fehlen` : ""}
                {abgemeldet > 0 ? ` · ${abgemeldet} abgemeldet` : ""}
              </span>

              {/* Ansagen nur, wenn Ansagen überhaupt eingerichtet sind — ohne
                  Sprecher passierte beim Klick nichts. Die Fehlt-Ansage
                  entfällt zusätzlich, wenn niemand fehlt (AK-C8). */}
              {announce.enabled && (
                <div className="flex items-center gap-1">
                  <button
                    disabled={busy}
                    onClick={() => ansagen(k.event_id, "deadline")}
                    className="flex items-center gap-1 rounded border border-slate-200 px-2 py-0.5 text-xs
                               text-slate-700 hover:bg-slate-50 disabled:opacity-40"
                    title="Ansage: noch N Minuten bis Anmeldeschluss"
                  >
                    <Megaphone className="h-3.5 w-3.5" aria-hidden />
                    Anmeldeschluss
                  </button>
                  {fehlend > 0 && (
                    <button
                      disabled={busy}
                      onClick={() => ansagen(k.event_id, "missing")}
                      className="flex items-center gap-1 rounded border border-slate-200 px-2 py-0.5 text-xs
                                 text-slate-700 hover:bg-slate-50 disabled:opacity-40"
                      title="Die fehlenden Spieler ansagen"
                    >
                      <Megaphone className="h-3.5 w-3.5" aria-hidden />
                      Fehlende
                    </button>
                  )}
                </div>
              )}

              <div className="ml-auto flex items-center gap-2 text-xs text-slate-600">
                <Clock className="h-3.5 w-3.5" aria-hidden />
                <label className="flex items-center gap-1">
                  Beginn
                  <input
                    type="datetime-local"
                    defaultValue={fuerEingabe(k.starts_at)}
                    disabled={busy}
                    onBlur={(e) =>
                      e.target.value !== fuerEingabe(k.starts_at) &&
                      zeiten(
                        k.event_id,
                        e.target.value,
                        fuerEingabe(k.closes_at),
                      )
                    }
                    className="rounded border border-slate-300 px-1 py-0.5"
                  />
                </label>
                <label className="flex items-center gap-1">
                  Schluss
                  <input
                    type="datetime-local"
                    defaultValue={fuerEingabe(k.closes_at)}
                    disabled={busy}
                    onBlur={(e) =>
                      e.target.value !== fuerEingabe(k.closes_at) &&
                      zeiten(
                        k.event_id,
                        fuerEingabe(k.starts_at),
                        e.target.value,
                      )
                    }
                    className="rounded border border-slate-300 px-1 py-0.5"
                  />
                </label>
              </div>
            </header>

            {aufgeklappt && (
              <ul className="divide-y divide-slate-100 border-t border-slate-100">
                {k.players.map((p) => {
                  const zust = spielerZustand(p);
                  const da = p.state === "checked_in";
                  return (
                    <li
                      key={p.player_id}
                      className="flex items-center gap-3 px-3 py-1.5 text-sm"
                    >
                      <span className="min-w-48 text-slate-800">
                        {p.first} {p.last}
                      </span>
                      {p.club && (
                        <span className="text-xs text-slate-400">{p.club}</span>
                      )}
                      <span className={`text-xs ${zust.klasse}`}>
                        {zust.text}
                      </span>

                      <div className="ml-auto flex gap-1">
                        {!da && (
                          <button
                            disabled={busy}
                            onClick={() =>
                              eingriff(k.event_id, p.player_id, "check_in")
                            }
                            className="flex items-center gap-1 rounded border border-slate-200 px-2 py-0.5 text-xs
                                       text-slate-700 hover:bg-slate-50 disabled:opacity-40"
                            title={
                              p.state === "withdrawn"
                                ? "Trotz Abmeldung als anwesend eintragen"
                                : "Als anwesend eintragen"
                            }
                          >
                            <CheckCircle2 className="h-3.5 w-3.5" aria-hidden />
                            da
                          </button>
                        )}
                        {da && (
                          <button
                            disabled={busy}
                            onClick={() =>
                              eingriff(k.event_id, p.player_id, "reset")
                            }
                            className="flex items-center gap-1 rounded border border-slate-200 px-2 py-0.5 text-xs
                                       text-slate-700 hover:bg-slate-50 disabled:opacity-40"
                            /* Zurücksetzen sperrt den Selbst-Check-In (B11) —
                               das steht im Titel, damit niemand überrascht ist,
                               wenn der Spieler danach nicht mehr klicken kann. */
                            title="Zurücksetzen — der Spieler kann sich danach nicht selbst wieder einchecken"
                          >
                            <RotateCcw className="h-3.5 w-3.5" aria-hidden />
                            zurücksetzen
                          </button>
                        )}
                        {p.locked && (
                          <button
                            disabled={busy}
                            onClick={() =>
                              eingriff(k.event_id, p.player_id, "unlock")
                            }
                            className="flex items-center gap-1 rounded border border-slate-200 px-2 py-0.5 text-xs
                                       text-slate-700 hover:bg-slate-50 disabled:opacity-40"
                            title="Sperre aufheben — der Spieler kann sich wieder selbst einchecken"
                          >
                            <Unlock className="h-3.5 w-3.5" aria-hidden />
                            entsperren
                          </button>
                        )}
                        {p.locked && (
                          <Lock
                            className="h-3.5 w-3.5 text-rose-400"
                            aria-label="gesperrt"
                          />
                        )}
                      </div>
                    </li>
                  );
                })}
                {k.players.length === 0 && (
                  <li className="px-3 py-2 text-xs text-slate-500">
                    Für diese Klasse liegt noch keine Meldung vor.
                  </li>
                )}
              </ul>
            )}
          </section>
        );
      })}
    </div>
  );
}
