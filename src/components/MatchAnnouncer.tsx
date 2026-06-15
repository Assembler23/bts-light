import { useEffect, useRef } from "react";
import { tabletOverview } from "../api";
import type {
  AnnounceConfig,
  AnnounceLanguageMode,
  CourtOverview,
} from "../types";
import {
  type AnnounceLang,
  type AnnounceMatchInput,
  cancelAnnouncements,
  playAnnouncement,
  resolveAnnouncementLanguage,
  unlockAudio,
} from "../io/announcer";

const POLL_MS = 2000;

// Gegen Doppel-Ansagen: pro Match-ID den letzten Ansage-Zeitpunkt merken.
// Modul-weit, damit ein StrictMode-Doppel-Mount oder überlappende Polls
// dasselbe Spiel nicht zweimal ansagen.
const lastAnnouncedAt = new Map<number, number>();
const DEBOUNCE_MS = 5000;

// Bestimmt die Ansagesprache. Im Auto-Modus: Englisch, wenn mindestens die
// Hälfte der Spieler auf dem Feld international ist (Nationalität gesetzt
// und ≠ GER) — Einzel ab 1 von 2, Doppel ab 2 von 4.
function resolveLanguage(
  court: CourtOverview,
  mode: AnnounceLanguageMode,
): AnnounceLang {
  return resolveAnnouncementLanguage(
    [...court.team1_nationalities, ...court.team2_nationalities],
    mode,
  );
}

interface Props {
  announce: AnnounceConfig;
}

/**
 * App-weiter, immer eingehängter Ansage-Detektor. Pollt die Felder-Übersicht
 * und sagt jedes Spiel an, das neu auf ein Feld gezogen wird (Gong → Feld →
 * Disziplin → Paarung → Feld). Rendert nichts.
 *
 * Der erste Poll ist nur die Baseline: bereits laufende Spiele werden nicht
 * nachträglich angesagt.
 */
export function MatchAnnouncer({ announce }: Props) {
  // CourtID → zuletzt gesehene Match-ID. Per CourtID, damit gleichnamige
  // Felder eines Mehr-Hallen-Turniers nicht denselben Eintrag teilen.
  const seenRef = useRef<Map<number, number>>(new Map());
  const baselineRef = useRef(false);
  const prevEnabledRef = useRef(announce.enabled);
  // Aktuelle Config in einer Ref, damit der Poll-Effekt stabil bleibt.
  const cfgRef = useRef(announce);
  cfgRef.current = announce;

  // Ansagen abgeschaltet → laufende Ansage sofort stoppen.
  useEffect(() => {
    if (prevEnabledRef.current && !announce.enabled) {
      cancelAnnouncements();
    }
    prevEnabledRef.current = announce.enabled;
  }, [announce.enabled]);

  // Einmaliger Klick-Listener: schaltet das WebView2-Audio für die Session
  // frei (der AudioContext startet sonst erst nach einer Nutzergeste).
  useEffect(() => {
    const unlock = () => unlockAudio();
    window.addEventListener("pointerdown", unlock, { once: true });
    return () => window.removeEventListener("pointerdown", unlock);
  }, []);

  useEffect(() => {
    let alive = true;
    const tick = () => {
      tabletOverview()
        .then((info) => {
          if (!alive) return;
          const cfg = cfgRef.current;
          const newMatches: CourtOverview[] = [];
          for (const court of info.courts) {
            const prev = seenRef.current.get(court.court_id) ?? 0;
            seenRef.current.set(court.court_id, court.match_id);
            if (
              baselineRef.current &&
              court.match_id !== 0 &&
              court.match_id !== prev
            ) {
              newMatches.push(court);
            }
          }
          // Erster Poll: nur Baseline füllen, nichts ansagen.
          if (!baselineRef.current) {
            baselineRef.current = true;
            return;
          }
          if (!cfg.enabled || newMatches.length === 0) return;

          const now = Date.now();
          for (const court of newMatches) {
            const last = lastAnnouncedAt.get(court.match_id) ?? 0;
            if (now - last < DEBOUNCE_MS) continue;
            lastAnnouncedAt.set(court.match_id, now);

            const lang = resolveLanguage(court, cfg.language_mode);
            const voiceURI = lang === "de" ? cfg.voice_de : cfg.voice_en;
            const input: AnnounceMatchInput = {
              courtLabel: court.court,
              discipline: court.discipline,
              teamANames: court.team1,
              teamBNames: court.team2,
            };
            // Strikt sequenziell über die globale Ansage-Warteschlange in
            // announcer.ts — kein Gong startet, während eine Ansage noch spricht.
            void playAnnouncement(input, lang, {
              rate: cfg.rate,
              voiceURI: voiceURI || undefined,
              gong: cfg.gong,
              nameOverrides: cfg.name_overrides,
            });
          }
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

  return null;
}
