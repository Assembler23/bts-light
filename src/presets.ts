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
}

const PUSH_URL = "https://badhub.de/api/live_update.php";

export const PRESETS: Preset[] = [
  {
    id: "bvbb",
    label: "BVBB – Badminton-Verband Berlin-Brandenburg",
    badhub: {
      url: PUSH_URL,
      password: "b09bc3e4334732191a999c8e",
      live_url: "https://badhub.de/live?t=bvbb",
    },
  },
  {
    id: "bvrp",
    label: "BVRP – Badminton-Verband Rheinland-Pfalz",
    badhub: {
      url: PUSH_URL,
      password: "a093735f59312450fdcd524a",
      live_url: "https://badhub.de/live?t=bvrp",
    },
  },
  {
    id: "hbv",
    label: "HBV – Hessischer Badminton-Verband",
    badhub: {
      url: PUSH_URL,
      password: "26514d25f567f024bbd74ba0",
      live_url: "https://badhub.de/live?t=hbv",
    },
  },
  {
    id: "bbv",
    label: "BBV – Badminton-Verband Bayern",
    badhub: {
      url: PUSH_URL,
      password: "5b33b5404f8940407064d437",
      live_url: "https://badhub.de/live?t=bbv",
    },
  },
  {
    id: "bwbv",
    label: "BWBV – Baden-Württembergischer Badminton-Verband",
    badhub: {
      url: PUSH_URL,
      password: "be5f04d712e0a412a880055f",
      live_url: "https://badhub.de/live?t=bwbv",
    },
  },
  {
    id: "nbv",
    label: "NBV – Niedersächsischer Badminton-Verband",
    badhub: {
      url: PUSH_URL,
      password: "2d25bb8a681de534d92ecbdc",
      live_url: "https://badhub.de/live?t=nbv",
    },
  },
];

export function findPreset(id: string): Preset | undefined {
  return PRESETS.find((p) => p.id === id);
}

/**
 * Kurzname des aktiven Ziels für die Kopfzeile – das Verbands-Kürzel (z. B.
 * „BVBB"), wenn die Config zu einem Preset passt, sonst „Eigenes Turnier".
 * Erkennung über die Live-URL bzw. das Passwort.
 */
export function tenantShortLabel(badhub: BadhubConfig): string {
  const preset = PRESETS.find(
    (p) =>
      (badhub.live_url && p.badhub.live_url === badhub.live_url) ||
      (badhub.password && p.badhub.password === badhub.password),
  );
  // Kürzel = Teil vor dem ersten „–" im Preset-Label.
  if (preset) return preset.label.split("–")[0].trim();
  return "Eigenes Turnier";
}
