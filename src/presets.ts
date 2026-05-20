// Vordefinierte Verbands-Presets für den Setup-Wizard.
//
// Ein Preset hinterlegt die Badhub-Zugangsdaten fest, damit ein
// Turnierleiter nur "BVBB" auswählen muss, statt URL und Passwort von Hand
// einzutragen. Das Push-Token ist verbandsweit und bewusst zum Einbau in
// die ausgelieferte App gedacht.

import type { BadhubConfig } from "./types";

export interface Preset {
  id: string;
  label: string;
  badhub: BadhubConfig;
  /** Öffentliche Live-Seite, nur zur Anzeige. */
  liveUrl: string;
}

export const PRESETS: Preset[] = [
  {
    id: "bvbb",
    label: "BVBB – Badminton-Verband Berlin-Brandenburg",
    badhub: {
      url: "https://badhub.de/api/live_update.php",
      password: "b09bc3e4334732191a999c8e",
    },
    liveUrl: "https://badhub.de/live?t=bvbb",
  },
];

export function findPreset(id: string): Preset | undefined {
  return PRESETS.find((p) => p.id === id);
}
