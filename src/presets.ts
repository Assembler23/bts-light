// Vordefinierte Verbands-Presets für den Setup-Wizard.
//
// Die hinterlegten Adressen zeigen immer auf das Produktivsystem; der
// Testsystem-Schalter im Setup biegt sie über `badhubZielFuer` um.
//
// Ein Preset hinterlegt die Badhub-Zugangsdaten fest, damit ein
// Turnierleiter nur "BVBB" auswählen muss, statt URL und Passwort von Hand
// einzutragen. Das Push-Token ist verbandsweit und bewusst zum Einbau in
// die ausgelieferte App gedacht.

import { badhubUrlFuer, istTestsystem } from "./io/badhubZiel.mjs";
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
      password: "d6dfe4f285dcdf53409e1876",
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
 * Das Preset zu einem gespeicherten Zugang – unabhängig davon, ob er auf
 * das Produktiv- oder das Testsystem zeigt. Ohne diese Normalisierung fällt
 * ein Testturnier in den Einstellungen auf „Anderes Turnier (manuell)"
 * zurück, und beim nächsten Speichern wäre die Verbandszuordnung weg.
 */
export function findPresetFor(badhub: BadhubConfig): Preset | undefined {
  const live = badhubUrlFuer(badhub.live_url, false);
  return PRESETS.find(
    (p) =>
      (live && p.badhub.live_url === live) ||
      (badhub.password && p.badhub.password === badhub.password),
  );
}

/**
 * Kurzname des aktiven Ziels für die Kopfzeile – das Verbands-Kürzel (z. B.
 * „BVBB"), wenn die Config zu einem Preset passt, sonst „Eigenes Turnier".
 * Erkennung über die Live-URL bzw. das Passwort.
 */
export function tenantShortLabel(badhub: BadhubConfig): string {
  const preset = findPresetFor(badhub);
  // Kürzel = Teil vor dem ersten „–" im Preset-Label.
  const name = preset ? preset.label.split("–")[0].trim() : "Eigenes Turnier";
  // Auf dem Testsystem gehört das in JEDE Kopfzeile: Wer hier ein echtes
  // Turnier fährt, sendet ins Leere – und merkt es sonst erst, wenn die
  // Halle nach dem Liveticker fragt.
  return istTestsystem(badhub.url) ? `${name} · TESTSYSTEM` : name;
}
