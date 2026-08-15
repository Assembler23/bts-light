// Manuelle Feld-Ansage für ein belegtes Feld (Gong + Feld + Disziplin +
// Paarung). Gemeinsam genutzt von der Ansagen-Seite und der Spielübersicht
// („nochmal aufrufen"). Sprache automatisch/konfiguriert; der auslösende Klick
// ist die User-Geste, die WebView2-Audio entsperrt.
import { playAnnouncement, resolveAnnouncementLanguage } from "./announcer";
import { azureOption } from "./azureAnnounce";
import type { AnnounceConfig, AzureTtsConfig, CourtOverview } from "../types";

/**
 * `side` benennt eine einzelne Partei (Nachruf „2. Aufruf für Partei A/B"
 * an der Feldkachel, Plan tl-liste-vereinfachen E1). Dann wird nur diese
 * gerufen — exakt wie beim Vorbereitungs-Nachruf je Partei: die genannte
 * Partei steht als `teamANames`, die andere bleibt leer, und auch die
 * Sprachwahl richtet sich nur nach den Nationen der genannten Partei.
 * Ohne `side` (bzw. mit `"both"`) bleibt es beim bisherigen Verhalten.
 */
export function announceCourt(
  court: CourtOverview,
  announce: AnnounceConfig,
  azureTts?: AzureTtsConfig,
  callStage: 1 | 2 | 3 = 1,
  side: "both" | "team1" | "team2" = "both",
): void {
  const nurEine = side === "team1" || side === "team2";
  const zweite = side === "team2";
  const namen = nurEine && zweite ? court.team2 : court.team1;
  const nats = nurEine
    ? (zweite ? court.team2_nationalities : court.team1_nationalities)
    : [...court.team1_nationalities, ...court.team2_nationalities];
  const lang = resolveAnnouncementLanguage(nats, announce.language_mode);
  void playAnnouncement(
    {
      courtLabel: court.court,
      discipline: court.discipline,
      className: court.class_label,
      teamANames: namen,
      // Bei einer einzelnen Partei bleibt die andere ungenannt — genau wie
      // beim Vorbereitungs-Nachruf.
      teamBNames: nurEine ? [] : court.team2,
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
