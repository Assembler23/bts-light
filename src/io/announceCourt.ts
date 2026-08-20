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
      // Zähltafelbedienung: Der Schalter entscheidet, nicht mehr die
      // Herkunft des Namens (ADR 0040 löst ADR 0007 ab). Wer eine Bedienung
      // am Feld stehen hat, will sie in der Regel auch gerufen hören —
      // gleich ob sie beim Aufruf zugewiesen wurde oder als pro-Feld-Hinweis
      // dort steht.
      scorekeeperNames: announce.announce_scorekeeper !== false
        ? court.scorekeeper
        : undefined,
      // Schiedsrichter/Aufschlagrichter: nur, was wirklich zugewiesen ist —
      // der Host liefert die Listen ohnehin leer, wenn ohne Schiedsrichter
      // gespielt wird (Spec Nr. 1).
      umpireNames: announce.announce_umpire !== false ? court.sr : undefined,
      serviceJudgeNames:
        announce.announce_umpire !== false ? court.ar : undefined,
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
 * „Feld X. Bitte mit dem Spielen beginnen." — die Aufforderung an ein
 * besetztes Feld, auf dem noch kein Punkt gefallen ist (Spec
 * `tl-sicht-feinschliff`, Punkt 3).
 *
 * Bewusst getrennt von [`announceCourt`]: Das ist **kein** Aufruf. Die
 * Paarung wurde längst gerufen, die Spieler stehen am Feld — noch einmal
 * „Feld 3. Herreneinzel A. Müller gegen Schmidt." hielte die Halle nur auf.
 *
 * Die Sprachwahl folgt den Nationen der Spieler dieses Felds, wie bei jeder
 * anderen Feld-Ansage auch.
 */
export function announceStartPlay(
  court: CourtOverview,
  announce: AnnounceConfig,
  azureTts?: AzureTtsConfig,
): void {
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
      startPlayOnly: true,
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
  // KEIN Schalter-Guard hier — das ist der ausdrückliche Knopf „Bedienung
  // nachrufen" aus der Spielübersicht bzw. TL-Web (ADR 0040: die
  // ausdrücklichen Knöpfe bleiben unberührt). Wer ihn drückt, sieht den Namen
  // davor und will genau diese Ansage; ihn stumm verfallen zu lassen, wäre
  // Bedienung ohne Rückmeldung.
  //
  // Der Aufrufer entscheidet also, WEN er ruft. Zu wissen ist dabei: Ohne
  // aktive Bediener-Verwaltung ist `court.scorekeeper` der
  // **pro-Feld-Hinweis**, also der Verlierer des zuletzt auf diesem Feld
  // beendeten Spiels — auf einem LAN-Ansage-Slave räumt der Sync-Lauf die
  // Zuweisungen und füllt den Hinweis weiter (Review 18.08.2026). In der
  // automatischen Feld-Ansage hängt genau das am Schalter
  // `announce_scorekeeper`; hier steht der Name auf dem Knopf.
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
