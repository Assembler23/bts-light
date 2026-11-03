// Baut die Azure-Option für die Ansage-Funktionen (`AnnounceOptions.azure`).
// Ist Azure-TTS aus, liefert sie `undefined` → die Ansage läuft wie bisher
// über Web Speech. `azureTtsSpeak` ruft den Rust-Command (Key bleibt im Backend).
import { azureTtsSpeak } from "../api";
import type { AnnounceOptions } from "./announcer";
import { cloudStimmeVollstaendig, dienstArt } from "./sprachdienst.mjs";
import type { AzureTtsConfig } from "../types";

export function azureOption(
  az: AzureTtsConfig | undefined,
): AnnounceOptions["azure"] {
  // Nur bei vollständiger Config (Key + Region) — sonst würde der Rust-
  // Command ohnehin ablehnen und die Ansage fiele still auf Web Speech
  // zurück (genau der stumme Slave-Bug vom Zwei-Hallen-Test).
  return az && cloudStimmeVollstaendig(az)
    ? {
        voice: az.voice,
        disciplineVoices: az.discipline_voices,
        synthesize: azureTtsSpeak,
        dienst: az.dienst || "azure",
      }
    : undefined;
}

export { cloudStimmeVollstaendig };

/** Azure-Option aus der vom Master geerbten Config (ADR 0003): Der Slave
 *  kennt nur die Stimme(n); Key/Region hält das Rust-Backend (`AppState`).
 *  Fallback, wenn die lokale Config unvollständig ist. `disciplineVoices` wird
 *  ebenfalls vom Master vererbt → die ferne Halle nutzt dieselbe Zuordnung. */
export function inheritedAzureOption(
  voice: string | null | undefined,
  disciplineVoices?: Record<string, string>,
  dienst?: string | null,
): AnnounceOptions["azure"] {
  // Bei ElevenLabs/OpenAI/Gemini schickt der Master keine Azure-Stimme mit
  // (leer) — die Vererbung gilt trotzdem.
  return voice || (voice != null && dienstArt(dienst) !== "azure")
    ? { voice: voice ?? "", disciplineVoices, synthesize: azureTtsSpeak, dienst: dienst || "azure" }
    : undefined;
}
