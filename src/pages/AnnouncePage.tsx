// Ansagen-Seite. Vorerst manuelle Feld-Ansage: für jedes Spiel, das gerade auf
// einem Feld steht, lässt sich die Hallen-Ansage (Gong + Feld + Disziplin +
// Paarung) per Knopfdruck auslösen. Die hochzählende Aufruf-Uhr und der 2./3.
// Aufruf (Call-Timer) bekommen hier später ihren Platz.
import { useEffect, useState } from "react";
import { Megaphone, Volume2 } from "lucide-react";
import { tabletOverview } from "../api";
import { CallTimerBadge } from "../components/CallTimerBadge";
import { announceCourt } from "../io/announceCourt";
import { playTestAnnouncement } from "../io/announcer";
import { useNow } from "../state/callTimer";
import type {
  AnnounceConfig,
  AzureTtsConfig,
  CallTimerConfig,
  CourtOverview,
} from "../types";
import { azureOption } from "../io/azureAnnounce";

const POLL_MS = 2500;

function teamsLabel(t1: string[], t2: string[]): string {
  return `${t1.join(" / ") || "—"} – ${t2.join(" / ") || "—"}`;
}

export function AnnouncePage({
  announce,
  callTimer,
  azureTts,
}: {
  announce: AnnounceConfig;
  callTimer: CallTimerConfig;
  azureTts?: AzureTtsConfig;
}) {
  const [courts, setCourts] = useState<CourtOverview[]>([]);
  const now = useNow();

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

  const onField = courts.filter((c) => c.match_id > 0);

  return (
    <main className="mx-auto flex min-h-full max-w-2xl flex-col gap-5 p-6 text-slate-800">
      <header>
        <h1 className="text-2xl font-semibold leading-tight">Ansagen</h1>
        <p className="text-sm text-slate-500">
          Feld-Ansagen manuell auslösen. Stimme, Sprache und Gong stellst du in
          den Einstellungen ein.
        </p>
      </header>

      <button
        onClick={() =>
          void playTestAnnouncement(announce.language_mode === "en" ? "en" : "de", {
            rate: announce.rate,
            voiceURI:
              (announce.language_mode === "en"
                ? announce.voice_en
                : announce.voice_de) || undefined,
            gong: announce.gong,
            nameOverrides: announce.name_overrides,
            nameOverridesEnabled: announce.name_overrides_enabled,
            azure: azureOption(azureTts),
          })
        }
        className="inline-flex w-fit items-center gap-2 rounded-lg bg-slate-100 px-3.5 py-2
                   text-sm font-medium text-slate-700 transition-colors hover:bg-slate-200"
      >
        <Volume2 size={16} /> Test-Ansage abspielen
      </button>

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
                onClick={() => announceCourt(c, announce, azureTts)}
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
    </main>
  );
}
