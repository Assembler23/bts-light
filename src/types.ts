// TypeScript-Spiegel der Rust-Strukturen (src-tauri/src/config.rs, commands.rs).

export interface BtpConfig {
  host: string;
  port: number;
  password: string | null;
}

export interface BadhubConfig {
  url: string;
  password: string;
  /** Öffentliche Live-Seite, z. B. https://badhub.de/live?t=bvbb */
  live_url: string;
}

export interface AppConfig {
  btp: BtpConfig;
  badhub: BadhubConfig;
  /** Opt-in: Diagnose-Logs automatisch an badhub.de hochladen. */
  upload_logs: boolean;
  /** Zufällige, dauerhafte Installations-ID (Frontend erzeugt sie). */
  install_id: string;
}

export interface SyncStatus {
  running: boolean;
  /** "idle" | "ok" | "btp_error" | "push_error" */
  kind: string;
  message: string;
  updated_at_ms: number;
}

/** Eine Court-Zeile der Felder-Übersicht (Rust: tablet::state::CourtOverview). */
export interface CourtOverview {
  court: string;
  /** Anzeigename des Matches, z. B. "HE G1"; leer wenn kein Match. */
  match_name: string;
  team1: string[];
  team2: string[];
  /** Satzstand als [Team1, Team2]-Paare. */
  sets: [number, number][];
  tablet_connected: boolean;
}

/** Tablet-Server-Adresse + Felder-Übersicht (Rust: commands::TabletInfo). */
export interface TabletInfo {
  server_host: string;
  courts: CourtOverview[];
}
