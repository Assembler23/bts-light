// Baut die Azure-Option für die Ansage-Funktionen (`AnnounceOptions.azure`).
// Ist Azure-TTS aus, liefert sie `undefined` → die Ansage läuft wie bisher
// über Web Speech. `azureTtsSpeak` ruft den Rust-Command (Key bleibt im Backend).
import { azureTtsSpeak } from "../api";
import type { AnnounceOptions } from "./announcer";
import type { AzureTtsConfig } from "../types";

export function azureOption(
  az: AzureTtsConfig | undefined,
): AnnounceOptions["azure"] {
  return az && az.enabled
    ? { voice: az.voice, synthesize: azureTtsSpeak }
    : undefined;
}
