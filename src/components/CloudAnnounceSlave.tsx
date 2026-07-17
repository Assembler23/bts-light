import { useEffect, useRef } from "react";
import { cloudAnnounceState } from "../api";
import { azureOption, inheritedAzureOption } from "../io/azureAnnounce";
import { playAnnouncement, playFreeText, resolveAnnouncementLanguage } from "../io/announcer";
import { setInheritedAzureVoice } from "../state/azureStatus";
import type { AnnounceConfig, AzureTtsConfig, Discipline } from "../types";

const POLL_MS = 3000;

/**
 * Cloud-Ansage-Slave (Mehr-Hallen über Cloud, B1a): Statt BTP zu lesen, holt
 * diese Instanz die Matches ihrer Halle + neue Freitexte aus dem Cloud-Relay
 * des Masters (`cloud_announce_state`) und sagt sie lokal an. Aktiv nur, wenn
 * die App als Cloud-Slave konfiguriert ist (slave_mode + master_namespace);
 * sonst liefert der Command leere Listen und hier passiert nichts.
 *
 * Erste Runde = Baseline: bereits laufende Matches / alte Freitexte werden
 * NICHT nachträglich angesagt (wie beim lokalen MatchAnnouncer).
 */
export function CloudAnnounceSlave({
  announce,
  azureTts,
}: {
  announce: AnnounceConfig;
  azureTts?: AzureTtsConfig;
}) {
  const cfgRef = useRef(announce);
  cfgRef.current = announce;
  const azureRef = useRef(azureTts);
  azureRef.current = azureTts;
  // CourtID → zuletzt gesehene Match-ID (für die Neu-Erkennung).
  const lastMatch = useRef<Map<number, number>>(new Map());
  const lastFreetextId = useRef(0);
  const baseline = useRef(false);

  useEffect(() => {
    let alive = true;
    const tick = () => {
      cloudAnnounceState(lastFreetextId.current)
        .then((state) => {
          if (!alive) return;
          const cfg = cfgRef.current;
          // Vererbungs-Status für die Ansage-Einstellungen publizieren
          // („vom Master geerbt ✓"). Null = keine Vererbung (kein Slave,
          // Azure am Master aus oder alter Relay).
          setInheritedAzureVoice(state.azure_voice);
          // Höchste Freitext-ID merken (Items sind bereits nur neue).
          if (state.freetext.length > 0) {
            lastFreetextId.current = state.freetext.reduce(
              (m, it) => Math.max(m, it.id),
              lastFreetextId.current,
            );
          }
          // Erste Runde nur als Baseline: aktuellen Stand merken, nichts ansagen.
          if (!baseline.current) {
            for (const c of state.courts) lastMatch.current.set(c.court_id, c.match_id);
            baseline.current = true;
            return;
          }
          if (!cfg.enabled) {
            // Trotzdem den Stand nachführen, damit nach dem Aktivieren nicht
            // alle laufenden Spiele nachträglich angesagt werden.
            for (const c of state.courts) lastMatch.current.set(c.court_id, c.match_id);
            return;
          }
          const lang = cfg.language_mode === "en" ? "en" : "de";
          const voiceURI = (lang === "de" ? cfg.voice_de : cfg.voice_en) || undefined;
          // Azure: vollständige lokale Config gewinnt, sonst die vom Master
          // geerbte (ADR 0003) — der Rust-Command wendet dieselbe Vorrangregel
          // auf Key/Region an.
          const azure =
            azureOption(azureRef.current) ?? inheritedAzureOption(state.azure_voice);
          // Neue Feld-Belegungen ansagen.
          for (const c of state.courts) {
            const prev = lastMatch.current.get(c.court_id);
            lastMatch.current.set(c.court_id, c.match_id);
            if (c.match_id <= 0 || c.match_id === prev) continue;
            const announceLang = resolveAnnouncementLanguage(
              [...c.team1_nationalities, ...c.team2_nationalities],
              cfg.language_mode,
            );
            void playAnnouncement(
              {
                courtLabel: c.court,
                discipline: c.discipline as Discipline,
                teamANames: c.team1,
                teamBNames: c.team2,
              },
              announceLang,
              {
                rate: cfg.rate,
                voiceURI:
                  (announceLang === "de" ? cfg.voice_de : cfg.voice_en) || undefined,
                gong: cfg.gong,
                nameOverrides: cfg.name_overrides,
                nameOverridesEnabled: cfg.name_overrides_enabled,
                azure,
              },
            );
          }
          // Neue Freitexte ansagen.
          for (const it of state.freetext) {
            void playFreeText(it.text, lang, {
              rate: cfg.rate,
              voiceURI,
              gong: cfg.gong,
              azure,
            });
          }
        })
        .catch(() => {});
    };
    tick();
    const id = window.setInterval(tick, POLL_MS);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, []);

  return null;
}
