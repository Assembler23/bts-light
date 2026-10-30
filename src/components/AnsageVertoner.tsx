import { useEffect, useRef } from "react";
import { ansageOffeneQuellen, ansageVertonen } from "../api";
import { jobZuAnsage } from "../io/announceJobs";
import { ansageOptionen } from "../io/announceCourt";
import { prepareFreeText } from "../io/announcer";
import { ansageAuftrag } from "../io/ansageTeilen";
import type { AnnounceConfig, AzureTtsConfig } from "../types";

const TAKT_MS = 1000;

/**
 * Vertoner der Ansage-Seiten im Browser (Spec `ansage-im-browser`, E1).
 *
 * Holt im Sekundentakt die TL-Aufträge und Freitexte, die der Kern für ihn
 * beansprucht hat — nur solche, deren Halle gerade eine Seite hört, jede
 * genau einmal —, baut daraus mit **derselben** Logik wie der PC-Sprecher
 * den Text und gibt ihn dem Kern zum Vertonen. Der Kern legt das Ergebnis
 * ab, die Seiten spielen es ab.
 *
 * Läuft bei „Audio für Ansage-Seiten erzeugen" — unabhängig davon, ob der PC
 * selbst spricht — und nie am Slave. Der Abruf ist zugleich das
 * Lebenszeichen, ohne das die Seiten „Turnier-PC erzeugt keine Ansagen"
 * zeigen.
 *
 * Desktop-Knöpfe gehen nicht hier durch: Sie geben ihre Ansage direkt beim
 * Drücken an die Seiten (`anSeitenGeben`).
 */
export function AnsageVertoner({
  announce,
  azureTts,
  slave,
}: {
  announce: AnnounceConfig;
  azureTts?: AzureTtsConfig;
  slave: boolean;
}) {
  const cfgRef = useRef(announce);
  cfgRef.current = announce;
  const azureRef = useRef(azureTts);
  azureRef.current = azureTts;
  const an = announce.page_audio && !slave;

  useEffect(() => {
    if (!an) return;
    let alive = true;
    // Ein Takt nach dem anderen: Azure braucht für eine Ansage gern eine
    // Sekunde, und zwei überlappende Takte brächten die Reihenfolge durcheinander.
    let laeuft = false;
    const tick = async () => {
      if (laeuft) return;
      laeuft = true;
      try {
        const quellen = await ansageOffeneQuellen();
        for (const job of quellen.jobs) {
          if (!alive) return;
          const r = await jobZuAnsage(job, cfgRef.current, azureRef.current);
          if (r) await ansageVertonen(ansageAuftrag(r.ansage, r.ziel)).catch(() => null);
        }
        for (const ft of quellen.freitexte) {
          if (!alive) return;
          const cfg = cfgRef.current;
          // Dieselbe Sprachwahl wie der Freitext-Sprecher am PC.
          const lang = cfg.language_mode === "en" ? "en" : "de";
          const p = prepareFreeText(ft.text, lang, ansageOptionen(cfg, lang, azureRef.current));
          if (p) {
            await ansageVertonen(
              ansageAuftrag(p, { art: "freetext", hall: ft.hall }),
            ).catch(() => null);
          }
        }
      } catch {
        /* Kern nicht erreichbar — nächster Takt */
      } finally {
        laeuft = false;
      }
    };
    void tick();
    const id = setInterval(() => void tick(), TAKT_MS);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [an]);

  return null;
}
