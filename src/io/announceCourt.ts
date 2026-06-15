// Manuelle Feld-Ansage für ein belegtes Feld (Gong + Feld + Disziplin +
// Paarung). Gemeinsam genutzt von der Ansagen-Seite und der Spielübersicht
// („nochmal aufrufen"). Sprache automatisch/konfiguriert; der auslösende Klick
// ist die User-Geste, die WebView2-Audio entsperrt.
import { playAnnouncement, resolveAnnouncementLanguage } from "./announcer";
import type { AnnounceConfig, CourtOverview } from "../types";

export function announceCourt(court: CourtOverview, announce: AnnounceConfig): void {
  const lang = resolveAnnouncementLanguage(
    [...court.team1_nationalities, ...court.team2_nationalities],
    announce.language_mode,
  );
  void playAnnouncement(
    {
      courtLabel: court.court,
      discipline: court.discipline,
      teamANames: court.team1,
      teamBNames: court.team2,
    },
    lang,
    {
      rate: announce.rate,
      voiceURI: lang === "de" ? announce.voice_de : announce.voice_en,
      gong: announce.gong,
      nameOverrides: announce.name_overrides,
    },
  );
}
