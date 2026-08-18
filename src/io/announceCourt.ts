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

/**
 * Nachruf an die Zähltafelbedienung eines Felds („Feld 3. Meier, bitte als
 * Tabletbedienung melden.") — der seit ADR 0007 offene Baustein, Spec
 * `tl-sicht-feinschliff` Punkt 2.
 *
 * Bewusst getrennt von [`announceCourt`]: Das ist **kein** Spieler-Aufruf.
 * Die Aufruf-Stufe der Spieler bleibt stehen; der Turnier-PC führt für die
 * Bedienung einen eigenen Zähler.
 *
 * Die Sprachwahl folgt den Nationen der **Spieler** dieses Felds — die
 * Bediener-Namen tragen keine Nation. Dieselbe Regel wie bei
 * [`announceOfficials`].
 */
export function announceScorekeeper(
  court: CourtOverview,
  names: string[],
  stage: 1 | 2 | 3,
  announce: AnnounceConfig,
  azureTts?: AzureTtsConfig,
): void {
  // NUR bei echter Zuweisung ansagen (ADR 0007) — dieselbe Prüfung wie in
  // `announceCourt` oben und im Cloud-Ansage-Slave. Ohne sie fiele
  // `court.scorekeeper` auf den reinen **pro-Feld-Hinweis** zurück: den
  // Verlierer des zuletzt auf diesem Feld beendeten Spiels. Der wäre nie
  // zugewiesen worden, und die Anlage riefe ihn trotzdem aus.
  //
  // Der Fall ist real, nicht theoretisch: Ein Ansage-Gerät baut seinen
  // Feld-Stand LOKAL auf. Auf einem LAN-Ansage-Slave mit ausgeschalteter
  // Bediener-Verwaltung — für einen reinen Ansage-PC der Normalfall —
  // räumt der Sync-Lauf die Zuweisungen, füllt den pro-Feld-Hinweis aber
  // weiter (Review 18.08.2026).
  if (!court.scorekeeper_assigned) return;
  if (names.length === 0) return;
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
      scorekeeperNames: names,
      callStage: stage,
      scorekeeperOnly: true,
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
