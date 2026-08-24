import { useEffect, useRef } from "react";
import {
  diffOccupiedCourts,
  type Stand as BaselineStand,
} from "../io/announceBaseline.mjs";
import { tabletOverview } from "../api";
import type {
  AnnounceConfig,
  AnnounceLanguageMode,
  AzureTtsConfig,
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
import { azureOption } from "../io/azureAnnounce";

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
  azureTts?: AzureTtsConfig;
}

/**
 * App-weiter, immer eingehängter Ansage-Detektor. Pollt die Felder-Übersicht
 * und sagt jedes Spiel an, das neu auf ein Feld gezogen wird (Gong → Feld →
 * Disziplin → Paarung → Feld). Rendert nichts.
 *
 * Der erste Poll ist nur die Baseline: bereits laufende Spiele werden nicht
 * nachträglich angesagt.
 */
export function MatchAnnouncer({ announce, azureTts }: Props) {
  // CourtID → zuletzt gesehene Match-ID. Per CourtID, damit gleichnamige
  // Felder eines Mehr-Hallen-Turniers nicht denselben Eintrag teilen.
  // Stand der Baseline-Logik (src/io/announceBaseline.mjs) — dort getestet,
  // weil der Fehler „beim Start werden alle laufenden Spiele angesagt" genau
  // hier saß (der erste Abruf kommt vor dem ersten BTP-Schnappschuss und ist
  // leer; als Baseline genommen galt danach jedes belegte Feld als neu).
  const standRef = useRef<BaselineStand>({
    baseline: new Map(),
    hatBaseline: false,
  });
  const prevEnabledRef = useRef(announce.enabled);
  // Aktuelle Config in einer Ref, damit der Poll-Effekt stabil bleibt.
  const cfgRef = useRef(announce);
  cfgRef.current = announce;
  // Azure-Config ebenfalls in einer Ref (Poll-Effekt hat leere Deps).
  const azureRef = useRef(azureTts);
  azureRef.current = azureTts;

  // Ansagen abgeschaltet → laufende Ansage sofort stoppen.
  useEffect(() => {
    if (prevEnabledRef.current && !announce.enabled) {
      cancelAnnouncements();
    }
    prevEnabledRef.current = announce.enabled;
  }, [announce.enabled]);

  // Ein Wechsel der Ansage-Halle setzt die gesehenen Stände NICHT zurück.
  //
  // Bis v0.9.255 wurde hier die Baseline geleert, damit „die erste Belegung
  // nach dem Wechsel angesagt wird". Die Folge war das Gegenteil von
  // hilfreich: Nach dem Umschalten galt **jedes** laufende Spiel der neu
  // hinzugekommenen Halle als frisch aufgerufen, und der Ansage-Slave rief
  // eine ganze Turnierhälfte nach (Befund 23.08.2026). Gewünscht ist „ab
  // sofort" — laufende Spiele bleiben stumm, neue werden gerufen.
  //
  // Ein eigener Reset ist dafür gar nicht nötig: `diffOccupiedCourts` führt
  // die Baseline **vor** dem Hallenfilter und merkt sich deshalb auch Felder
  // der gefilterten Halle — genau dafür (siehe dortigen Kommentar und
  // `scripts/test-announce-baseline.mjs`, „Halle hinzunehmen holt nichts
  // nach"). Nach dem Umschalten sind alle laufenden Spiele bereits bekannt.
  //
  // Dieselbe Regel wie bei der Schiedsrichter-Rotation seit v0.9.253: Der
  // erste beobachtete Stand ist Ausgangslage, kein Ereignis.

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
          // Mehr-Hallen-Turnier: nur die eingestellte Halle ansagen (leer = alle).
          // So hört in einem 2-Hallen-Setup jede Halle/Instanz nur ihre Spiele.
          const { neue, stand } = diffOccupiedCourts(
            standRef.current,
            info.courts,
            cfg.announce_hall || "",
          );
          standRef.current = stand;
          const newMatches = neue as CourtOverview[];
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
              className: court.class_label,
              teamANames: court.team1,
              teamBNames: court.team2,
              roundName: court.round_name,
              // Bedienung und Schiedsrichter hängen an ihren Schaltern
              // (ADR 0040). Beide fehlten hier bisher ganz bzw. nur bei
              // echter Zuweisung, während der manuelle Aufruf und jeder
              // Nachruf sie über `announceCourt` längst mitgaben — dieselbe
              // Belegung wurde also je nach Auslöser anders angesagt.
              scorekeeperNames:
                cfg.announce_scorekeeper !== false
                  ? court.scorekeeper
                  : undefined,
              umpireNames: cfg.announce_umpire !== false ? court.sr : undefined,
              serviceJudgeNames:
                cfg.announce_umpire !== false ? court.ar : undefined,
            };
            // Strikt sequenziell über die globale Ansage-Warteschlange in
            // announcer.ts — kein Gong startet, während eine Ansage noch spricht.
            void playAnnouncement(input, lang, {
              rate: cfg.rate,
              voiceURI: voiceURI || undefined,
              gong: cfg.gong,
              nameOverrides: cfg.name_overrides,
              nameOverridesEnabled: cfg.name_overrides_enabled,
              azure: azureOption(azureRef.current),
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
