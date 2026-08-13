// Manuelle Feld-Ansage für ein belegtes Feld (Gong + Feld + Disziplin +
// Paarung). Gemeinsam genutzt von der Ansagen-Seite und der Spielübersicht
// („nochmal aufrufen"). Sprache automatisch/konfiguriert; der auslösende Klick
// ist die User-Geste, die WebView2-Audio entsperrt.
import { playAnnouncement, resolveAnnouncementLanguage } from "./announcer";
import { azureOption } from "./azureAnnounce";
import type { AnnounceConfig, AzureTtsConfig, CourtOverview } from "../types";

export function announceCourt(
  court: CourtOverview,
  announce: AnnounceConfig,
  azureTts?: AzureTtsConfig,
  callStage: 1 | 2 | 3 = 1,
): void {
  const lang = resolveAnnouncementLanguage(
    [...court.team1_nationalities, ...court.team2_nationalities],
    announce.language_mode,
  );
  void playAnnouncement(
    {
      courtLabel: court.court,
      discipline: court.discipline,
      className: court.class_label,
      teamANames: court.team1,
      teamBNames: court.team2,
      roundName: court.round_name,
      // Zähltafelbediener nur ansagen, wenn er zugewiesen wurde (ADR 0007) —
      // nicht der reine pro-Feld-Hinweis.
      scorekeeperNames: court.scorekeeper_assigned
        ? court.scorekeeper
        : undefined,
      // Schiedsrichter/Aufschlagrichter: nur, was wirklich zugewiesen ist —
      // der Host liefert die Listen ohnehin leer, wenn ohne Schiedsrichter
      // gespielt wird (Spec Nr. 1).
      umpireNames: court.sr,
      serviceJudgeNames: court.ar,
      callStage,
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

/**
 * Nur die Besetzung eines Felds ansagen (manueller Knopf „SR/AR ansagen",
 * Spec schiedsrichter-management Nr. 8).
 *
 * Bewusst getrennt von [`announceCourt`]: Eine nachträgliche Zuweisung soll
 * nicht die ganze Paarung erneut aufrufen — das Spiel läuft schon.
 */
export function announceOfficials(
  court: CourtOverview,
  announce: AnnounceConfig,
  azureTts?: AzureTtsConfig,
): void {
  if (court.sr.length === 0 && court.ar.length === 0) return;
  const lang = resolveAnnouncementLanguage(
    [...court.team1_nationalities, ...court.team2_nationalities],
    announce.language_mode,
  );
  void playAnnouncement(
    {
      courtLabel: court.court,
      discipline: court.discipline,
      teamANames: [],
      teamBNames: [],
      umpireNames: court.sr,
      serviceJudgeNames: court.ar,
      officialsOnly: true,
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
