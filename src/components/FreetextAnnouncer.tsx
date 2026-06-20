import { useEffect, useRef } from "react";
import { pendingFreetext } from "../api";
import { azureOption } from "../io/azureAnnounce";
import { playFreeText } from "../io/announcer";
import type { AnnounceConfig, AzureTtsConfig } from "../types";

const POLL_MS = 3000;

/**
 * App-weiter Poller für manuelle Freitext-Ansagen. Master legt Freitexte über
 * `publish_freetext` ab; Master und Slaves holen sie über `pending_freetext`
 * (Slave: vom Master) und sagen die für ihre Halle bestimmten an.
 *
 * Erste Runde ist nur Baseline: vorhandene (alte) Freitexte werden NICHT
 * nachträglich angesagt — z. B. nach einem Slave-Neustart.
 */
export function FreetextAnnouncer({
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
  const lastIdRef = useRef(0);
  const baselineRef = useRef(false);

  useEffect(() => {
    let alive = true;
    const tick = () => {
      pendingFreetext(lastIdRef.current)
        .then((items) => {
          if (!alive) return;
          // `items` sind bereits nur neue (id > lastId). Höchste id merken.
          if (items.length > 0) {
            lastIdRef.current = items.reduce(
              (m, it) => Math.max(m, it.id),
              lastIdRef.current,
            );
          }
          // Erste Runde = Baseline: alte Freitexte als gesehen markieren, nicht
          // ansagen.
          if (!baselineRef.current) {
            baselineRef.current = true;
            return;
          }
          const cfg = cfgRef.current;
          if (!cfg.enabled || items.length === 0) return;
          const lang = cfg.language_mode === "en" ? "en" : "de";
          const voiceURI =
            (lang === "de" ? cfg.voice_de : cfg.voice_en) || undefined;
          for (const it of items) {
            void playFreeText(it.text, lang, {
              rate: cfg.rate,
              voiceURI,
              gong: cfg.gong,
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
