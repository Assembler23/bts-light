// Baut die Azure-Option für die Ansage-Funktionen (`AnnounceOptions.azure`).
// Ist Azure-TTS aus, liefert sie `undefined` → die Ansage läuft wie bisher
// über Web Speech. `azureTtsSpeak` ruft den Rust-Command (Key bleibt im Backend).
import { azureTtsSpeak } from "../api";
import type { AnnounceOptions } from "./announcer";
import type { AzureTtsConfig } from "../types";

export function azureOption(
  az: AzureTtsConfig | undefined,
): AnnounceOptions["azure"] {
  // Nur bei vollständiger Config (Key + Region) — sonst würde der Rust-
  // Command ohnehin ablehnen und die Ansage fiele still auf Web Speech
  // zurück (genau der stumme Slave-Bug vom Zwei-Hallen-Test).
  return az && az.enabled && az.key && az.region
    ? {
        voice: az.voice,
        disciplineVoices: az.discipline_voices,
        synthesize: azureTtsSpeak,
      }
    : undefined;
}

/** Azure-Option aus der vom Master geerbten Config (ADR 0003): Der Slave
 *  kennt nur die Stimme; Key/Region hält das Rust-Backend (`AppState`).
 *  Fallback, wenn die lokale Config unvollständig ist. */
export function inheritedAzureOption(
  voice: string | null | undefined,
): AnnounceOptions["azure"] {
  return voice ? { voice, synthesize: azureTtsSpeak } : undefined;
}
