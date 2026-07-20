// Ansagen-Seite. Manuelle Feld-Ansage (für Spiele, die gerade auf einem Feld
// stehen), Freitext-Ansage (Master → Halle/„alle"), gespeicherte Ansage-Blöcke
// für wiederkehrende Ansagen und ein Verlauf der letzten zehn manuell/Freitext
// ausgelösten Ansagen mit „Erneut abspielen". Alle Ansage-Detail-Einstellungen
// liegen unten im Abschnitt „Ansage-Einstellungen".
import { useEffect, useState } from "react";
import {
  Bookmark,
  Megaphone,
  Pencil,
  RotateCcw,
  Save,
  Trash2,
  Volume2,
} from "lucide-react";
import { publishFreetext, saveConfig, tabletOverview } from "../api";
import { AnnounceSettings } from "../components/AnnounceSettings";
import { CallTimerBadge } from "../components/CallTimerBadge";
import { announceCourt } from "../io/announceCourt";
import {
  disciplineWithClass,
  playPreparationAnnouncement,
  playTestAnnouncement,
  resolveAnnouncementLanguage,
} from "../io/announcer";
import { useNow } from "../state/callTimer";
import { usePreparedGames } from "../state/preparedGames";
import { recordAnnounce, useAnnounceHistory } from "../state/announceHistory";
import type {
  AnnounceConfig,
  AppConfig,
  AzureTtsConfig,
  CallTimerConfig,
  CloudPrepared,
  CourtOverview,
  Discipline,
} from "../types";
import { azureOption } from "../io/azureAnnounce";

const POLL_MS = 2500;

function teamsLabel(t1: string[], t2: string[]): string {
  return `${t1.join(" / ") || "—"} – ${t2.join(" / ") || "—"}`;
}

function fieldEntryText(c: CourtOverview): string {
  return `Feld ${c.court}: ${teamsLabel(c.team1, c.team2)}`;
}

/** „vor X Min." (bzw. „gerade eben") seit dem Aufruf-Zeitstempel. */
function sinceLabel(calledAtMs: number, now: number): string {
  const min = Math.floor((now - calledAtMs) / 60000);
  if (min < 1) return "gerade eben";
  return `vor ${min} Min.`;
}

/** Nachname (letztes Wort) als kurzes Knopf-Label; Fallback: ganzer Name. */
function shortName(team: string[]): string {
  const first = team[0] ?? "";
  return first.trim().split(" ").filter(Boolean).slice(-1)[0] || first;
}

function timeLabel(ts: number): string {
  try {
    return new Date(ts).toLocaleTimeString("de-DE", {
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "";
  }
}

export function AnnouncePage({
  announce,
  callTimer,
  azureTts,
  config,
  onConfigSaved,
}: {
  announce: AnnounceConfig;
  callTimer: CallTimerConfig;
  azureTts?: AzureTtsConfig;
  /** Volle Konfiguration + Rückmeldung beim Speichern – für die
   *  Ansage-Einstellungen und die gespeicherten Ansage-Blöcke. */
  config: AppConfig;
  onConfigSaved: (config: AppConfig) => void;
}) {
  const [courts, setCourts] = useState<CourtOverview[]>([]);
  // Aufgerufene Spiele der eigenen Halle (nur Cloud-Slave, Cluster C Stufe 2):
  // vom CloudAnnounceSlave-Poll veröffentlicht (ein Poll, zwei Verbraucher).
  const prepared = usePreparedGames();
  // MatchID:Seite → bereits erfolgter Aufruf (2 nach dem ersten Nachruf) →
  // der nächste wird zum „Dritten und letzten Aufruf" (wie am Master).
  const [callStages, setCallStages] = useState<Map<string, 2 | 3>>(new Map());
  const now = useNow();
  // Freitext-Ansage (Master → eigene Halle/„alle"; Slaves holen sie ab).
  const [freeText, setFreeText] = useState("");
  const [freeHall, setFreeHall] = useState(""); // "" = alle Hallen
  const [freeSent, setFreeSent] = useState(false);
  const history = useAnnounceHistory();
  const savedBlocks = config.announce.saved_announcements ?? [];

  useEffect(() => {
    let active = true;
    const tick = () => {
      tabletOverview()
        .then((i) => {
          if (active) setCourts(i.courts);
        })
        .catch(() => {});
    };
    tick();
    const id = setInterval(tick, POLL_MS);
    return () => {
      active = false;
      clearInterval(id);
    };
  }, []);

  // Nachruf-Zähler bereinigen, sobald ein Spiel nicht mehr aufgerufen ist
  // (zurückgenommen / aufs Feld gerufen / beendet). Sonst startete ein später
  // erneut gerufenes Match direkt beim „Dritten Aufruf" — der Master räumt
  // seinen Zähler in `retract()` analog auf (Konsistenz beider Rollen).
  useEffect(() => {
    setCallStages((prev) => {
      if (prev.size === 0) return prev;
      const live = new Set(prepared.map((p) => p.match_id));
      let changed = false;
      const next = new Map(prev);
      for (const key of next.keys()) {
        if (!live.has(Number(key.split(":")[0]))) {
          next.delete(key);
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [prepared]);

  const onField = courts.filter((c) => c.match_id > 0);
  const halls = [
    ...new Set(courts.map((c) => c.location).filter((l) => l !== "")),
  ].sort((a, b) => a.localeCompare(b, "de"));

  function flashSent() {
    setFreeSent(true);
    window.setTimeout(() => setFreeSent(false), 3000);
  }

  // Freitext (oder einen Block) in einer Halle/allen ansagen + protokollieren.
  // Liefert true bei Erfolg, damit der Aufrufer das Textfeld nur dann leert.
  async function announceText(text: string, hall: string): Promise<boolean> {
    const t = text.trim();
    if (!t) return false;
    try {
      await publishFreetext(hall, t);
      recordAnnounce({ kind: "freetext", text: t, hall });
      flashSent();
      return true;
    } catch {
      // Senden fehlgeschlagen – Text bleibt stehen, erneut versuchen.
      return false;
    }
  }

  async function sendFreetext() {
    const t = freeText.trim();
    if (!t) return;
    if (await announceText(t, freeHall)) setFreeText("");
  }

  // Gezielter Zweit-/Drittaufruf NUR einer Partei eines aufgerufenen Spiels –
  // am Slave-PC, lokal in der eigenen Halle angesagt (Cluster C Stufe 2).
  // Erster Nachruf = „Zweiter Aufruf", jeder weitere = „Dritter und letzter".
  function secondCall(p: CloudPrepared, side: "a" | "b") {
    const key = `${p.match_id}:${side}`;
    const stage: 2 | 3 = callStages.get(key) ? 3 : 2;
    setCallStages((m) => new Map(m).set(key, stage));
    const names = side === "a" ? p.team1 : p.team2;
    const nats = side === "a" ? p.team1_nationalities : p.team2_nationalities;
    const lang = resolveAnnouncementLanguage(nats, announce.language_mode);
    void playPreparationAnnouncement(
      {
        discipline: (p.discipline || "unknown") as Discipline,
        className: p.class_label,
        teamANames: names,
        teamBNames: [], // nur die fehlende Partei ansagen
        hall: p.hall || undefined,
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
  }

  // Manuelle Feld-Ansage (lokal am Steuer-PC) + protokollieren.
  function announceField(c: CourtOverview) {
    announceCourt(c, announce, azureTts);
    recordAnnounce({
      kind: "field",
      text: fieldEntryText(c),
      hall: c.location,
      court: c,
    });
  }

  // Aktuellen Freitext als wiederkehrenden Block speichern (dedupliziert).
  async function saveBlock() {
    const t = freeText.trim();
    if (!t || savedBlocks.includes(t)) return;
    const next: AppConfig = {
      ...config,
      announce: {
        ...config.announce,
        saved_announcements: [...savedBlocks, t],
      },
    };
    try {
      await saveConfig(next);
      onConfigSaved(next);
    } catch {
      /* Speichern fehlgeschlagen */
    }
  }

  async function removeBlock(text: string) {
    const next: AppConfig = {
      ...config,
      announce: {
        ...config.announce,
        saved_announcements: savedBlocks.filter((b) => b !== text),
      },
    };
    try {
      await saveConfig(next);
      onConfigSaved(next);
    } catch {
      /* Löschen fehlgeschlagen */
    }
  }

  function replay(entry: (typeof history)[number]) {
    if (entry.kind === "freetext") {
      void announceText(entry.text, entry.hall);
    } else if (entry.court) {
      announceField(entry.court);
    }
  }

  const blockSaved =
    freeText.trim() !== "" && savedBlocks.includes(freeText.trim());

  return (
    <main className="mx-auto flex min-h-full max-w-2xl flex-col gap-5 p-6 text-slate-800">
      <header>
        <h1 className="text-2xl font-semibold leading-tight">Ansagen</h1>
        <p className="text-sm text-slate-500">
          Feld-Ansagen manuell auslösen. Stimme, Sprache und Gong stellst du
          unten ein.
        </p>
      </header>

      <button
        onClick={() =>
          void playTestAnnouncement(
            announce.language_mode === "en" ? "en" : "de",
            {
              rate: announce.rate,
              voiceURI:
                (announce.language_mode === "en"
                  ? announce.voice_en
                  : announce.voice_de) || undefined,
              gong: announce.gong,
              nameOverrides: announce.name_overrides,
              nameOverridesEnabled: announce.name_overrides_enabled,
              azure: azureOption(azureTts),
            },
          )
        }
        className="inline-flex w-fit items-center gap-2 rounded-lg bg-slate-100 px-3.5 py-2
                   text-sm font-medium text-slate-700 transition-colors hover:bg-slate-200"
      >
        <Volume2 size={16} /> Test-Ansage abspielen
      </button>

      {/* Freitext-Ansage: eintippen → in einer Halle oder allen ansagen. */}
      <section className="flex flex-col gap-2 rounded-xl border border-slate-200 bg-white p-4">
        <h2 className="text-sm font-semibold text-slate-700">
          Freitext-Ansage
        </h2>
        <p className="text-xs text-slate-500">
          Text eintippen und in einer Halle oder allen Hallen ansagen (eigener
          Gong, Stimme wie unten eingestellt). Slave-Rechner holen die Ansage
          vom Master.
        </p>
        <textarea
          value={freeText}
          onChange={(e) => setFreeText(e.currentTarget.value)}
          rows={2}
          placeholder="z. B. „Die Siegerehrung beginnt in 10 Minuten in Halle 1.“"
          className="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm
                     focus:border-slate-500 focus:outline-none"
        />
        <div className="flex flex-wrap items-center gap-2">
          {halls.length >= 2 && (
            <select
              value={freeHall}
              onChange={(e) => setFreeHall(e.currentTarget.value)}
              className="rounded-lg border border-slate-300 bg-white px-2 py-2 text-sm"
            >
              <option value="">Alle Hallen</option>
              {halls.map((h) => (
                <option key={h} value={h}>
                  {h}
                </option>
              ))}
            </select>
          )}
          <button
            onClick={() => void sendFreetext()}
            disabled={!freeText.trim() || !announce.enabled}
            className="inline-flex items-center gap-2 rounded-lg bg-slate-800 px-3.5 py-2 text-sm
                       font-medium text-white transition-colors hover:bg-slate-700
                       disabled:cursor-not-allowed disabled:opacity-50"
          >
            <Megaphone size={16} /> Ansagen
          </button>
          <button
            onClick={() => void saveBlock()}
            disabled={!freeText.trim() || blockSaved}
            title={
              blockSaved
                ? "Dieser Text ist bereits als Block gespeichert"
                : "Diesen Text als wiederkehrende Ansage speichern"
            }
            className="inline-flex items-center gap-2 rounded-lg bg-slate-100 px-3.5 py-2 text-sm
                       font-medium text-slate-700 transition-colors hover:bg-slate-200
                       disabled:cursor-not-allowed disabled:opacity-50"
          >
            <Save size={16} />{" "}
            {blockSaved ? "Gespeichert" : "Als Block speichern"}
          </button>
          {!announce.enabled && (
            <span className="text-xs text-amber-600">
              Ansagen sind unten deaktiviert.
            </span>
          )}
          {freeSent && (
            <span className="text-xs font-medium text-emerald-600">
              Gesendet ✓
            </span>
          )}
        </div>
      </section>

      {/* Gespeicherte Ansage-Blöcke (wiederkehrende Ansagen). */}
      {savedBlocks.length > 0 && (
        <section className="flex flex-col gap-2 rounded-xl border border-slate-200 bg-white p-4">
          <h2 className="flex items-center gap-1.5 text-sm font-semibold text-slate-700">
            <Bookmark size={15} /> Gespeicherte Ansagen
          </h2>
          <div className="flex flex-col gap-1.5">
            {savedBlocks.map((block) => (
              <div
                key={block}
                className="flex items-center gap-2 rounded-lg border border-slate-200 bg-slate-50 px-3 py-2"
              >
                <span
                  className="min-w-0 flex-1 truncate text-sm text-slate-700"
                  title={block}
                >
                  {block}
                </span>
                <button
                  onClick={() => void announceText(block, freeHall)}
                  disabled={!announce.enabled}
                  title="Diesen Block jetzt ansagen"
                  className="inline-flex shrink-0 items-center gap-1.5 rounded-lg bg-slate-800 px-3 py-1.5
                             text-sm font-medium text-white transition-colors hover:bg-slate-700
                             disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <Megaphone size={14} /> Ansagen
                </button>
                <button
                  onClick={() => setFreeText(block)}
                  title="In das Textfeld laden (bearbeiten)"
                  className="shrink-0 rounded-lg bg-slate-100 p-1.5 text-slate-600 hover:bg-slate-200"
                >
                  <Pencil size={15} />
                </button>
                <button
                  onClick={() => void removeBlock(block)}
                  title="Block löschen"
                  className="shrink-0 rounded-lg bg-slate-100 p-1.5 text-slate-500 hover:bg-rose-100 hover:text-rose-700"
                >
                  <Trash2 size={15} />
                </button>
              </div>
            ))}
          </div>
        </section>
      )}

      {/* Cloud-Slave: aufgerufene Spiele der eigenen Halle (vom Master gepusht)
          mit gezieltem Zweit-/Drittaufruf je fehlender Partei, lokal angesagt
          (Cluster C Stufe 2). */}
      {config.slave_mode && (
        <section className="flex flex-col gap-2">
          <h2 className="text-sm font-semibold text-slate-700">
            Aufgerufene Spiele{" "}
            <span className="text-slate-400">({prepared.length})</span>
          </h2>
          <p className="text-xs text-slate-500">
            Von der Turnierleitung „in Vorbereitung" gerufene Spiele deiner
            Halle. Fehlt eine Partei, kannst du sie hier gezielt nachrufen – die
            Ansage läuft lokal auf diesem Rechner.
          </p>
          {prepared.length === 0 ? (
            <div className="rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-400">
              Aktuell wurde kein Spiel in die Vorbereitung gerufen.
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              {prepared.map((p) => (
                <div
                  key={p.match_id}
                  className="flex flex-wrap items-center gap-2 rounded-xl border border-slate-200 bg-white px-4 py-3"
                >
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm font-medium">
                      {[
                        disciplineWithClass(
                          (p.discipline || "unknown") as Discipline,
                          p.class_label,
                          announce.language_mode === "en" ? "en" : "de",
                        ),
                        p.round_name,
                      ]
                        .filter(Boolean)
                        .join(" · ") || "Spiel"}
                    </div>
                    <div className="truncate text-xs text-slate-500">
                      {p.team1.length > 0 ? p.team1.join(" / ") : "—"}{" "}
                      <span className="text-slate-400">gegen</span>{" "}
                      {p.team2.length > 0 ? p.team2.join(" / ") : "—"}
                    </div>
                  </div>
                  <span className="shrink-0 text-xs text-sky-700">
                    {sinceLabel(p.called_at_ms, now)}
                  </span>
                  {/* Nachruf je Partei; nur wenn Ansagen aktiv sind. */}
                  {announce.enabled &&
                    (["a", "b"] as const).map((side) => {
                      const team = side === "a" ? p.team1 : p.team2;
                      if (team.length === 0) return null;
                      const nextStage = callStages.get(`${p.match_id}:${side}`)
                        ? 3
                        : 2;
                      return (
                        <button
                          key={side}
                          onClick={() => secondCall(p, side)}
                          title={`${nextStage === 3 ? "Dritter und letzter" : "Zweiter"} Aufruf für ${team.join(" / ")}`}
                          className="inline-flex shrink-0 items-center gap-1 rounded-md
                                     bg-amber-100 px-2 py-1.5 text-xs font-medium text-amber-800
                                     transition-colors hover:bg-amber-200"
                        >
                          {side === "a" ? "◂" : "▸"} {nextStage}. Ruf{" "}
                          {shortName(team)}
                        </button>
                      );
                    })}
                  {!announce.enabled && (
                    <span className="shrink-0 text-xs text-amber-600">
                      Ansagen unten deaktiviert.
                    </span>
                  )}
                </div>
              ))}
            </div>
          )}
        </section>
      )}

      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-semibold text-slate-700">
          Spiele auf den Feldern{" "}
          <span className="text-slate-400">({onField.length})</span>
        </h2>
        {onField.length === 0 && (
          <div className="rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-400">
            Aktuell steht kein Spiel auf einem Feld.
          </div>
        )}
        <div className="flex flex-col gap-2">
          {onField.map((c) => (
            <div
              key={c.court_id}
              className="flex items-center gap-3 rounded-xl border border-slate-200 bg-white px-4 py-3"
            >
              <span className="flex h-9 min-w-9 items-center justify-center rounded-lg bg-slate-800 px-2 text-sm font-semibold text-white">
                {c.court}
              </span>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="truncate text-sm font-medium">
                    {c.match_name || "Spiel"}
                  </span>
                  {callTimer.enabled && c.on_court_since_ms != null && (
                    <CallTimerBadge
                      sinceMs={c.on_court_since_ms}
                      now={now}
                      cfg={callTimer}
                    />
                  )}
                </div>
                <div className="truncate text-xs text-slate-500">
                  {teamsLabel(c.team1, c.team2)}
                </div>
              </div>
              <button
                onClick={() => announceField(c)}
                title="Dieses Feld ansagen"
                className="inline-flex shrink-0 items-center gap-1.5 rounded-lg bg-slate-100 px-3 py-1.5
                           text-sm font-medium text-slate-700 transition-colors hover:bg-slate-200"
              >
                <Megaphone size={15} /> Ansagen
              </button>
            </div>
          ))}
        </div>
      </section>

      {/* Verlauf der letzten zehn manuell/Freitext ausgelösten Ansagen. */}
      {history.length > 0 && (
        <section className="flex flex-col gap-2">
          <h2 className="flex items-center gap-1.5 text-sm font-semibold text-slate-700">
            <RotateCcw size={15} /> Letzte Ansagen
          </h2>
          <div className="flex flex-col gap-1.5">
            {history.map((entry) => (
              <div
                key={entry.id}
                className="flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2"
              >
                <span
                  className="shrink-0 rounded bg-slate-100 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-slate-500"
                  title={entry.kind === "field" ? "Feld-Ansage" : "Freitext"}
                >
                  {entry.kind === "field" ? "Feld" : "Text"}
                </span>
                <div className="min-w-0 flex-1">
                  <div
                    className="truncate text-sm text-slate-700"
                    title={entry.text}
                  >
                    {entry.text}
                  </div>
                  <div className="text-xs text-slate-400">
                    {timeLabel(entry.ts)}
                    {entry.hall ? ` · ${entry.hall}` : ""}
                  </div>
                </div>
                <button
                  onClick={() => replay(entry)}
                  disabled={
                    !announce.enabled ||
                    (entry.kind === "field" && !entry.court)
                  }
                  title="Diese Ansage erneut abspielen"
                  className="inline-flex shrink-0 items-center gap-1.5 rounded-lg bg-slate-100 px-3 py-1.5
                             text-sm font-medium text-slate-700 transition-colors hover:bg-slate-200
                             disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <RotateCcw size={14} /> Erneut
                </button>
              </div>
            ))}
          </div>
        </section>
      )}

      {/* Alle Ansage-Einstellungen (Sprache, Stimmen, Tempo, Gong, Aussprache,
          Halle, Azure) — in den Einstellungen wird das Modul nur an-/ausgeschaltet. */}
      <AnnounceSettings config={config} onSaved={onConfigSaved} />
    </main>
  );
}
