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

/** Verbindungsart für die Schiedsrichter-Tablets. */
export type ConnectionMode = "lan" | "cloud";

export interface AppConfig {
  btp: BtpConfig;
  badhub: BadhubConfig;
  /** Opt-in: Diagnose-Logs automatisch an badhub.de hochladen. */
  upload_logs: boolean;
  /** Zufällige, dauerhafte Installations-ID (Frontend erzeugt sie). */
  install_id: string;
  /** Verbindungsart für die Tablets: LAN (lokal) oder Cloud (über badhub.de). */
  connection_mode: ConnectionMode;
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
  /** Akkustand des Tablets (Android/Chrome) – null bei iPads/Safari. */
  battery: { percent: number; charging: boolean } | null;
  /** Verletzung/Behandlung läuft an diesem Court. */
  injury: boolean;
  /** Die Turnierleitung wurde an diesen Court gerufen. */
  official_call: boolean;
}

/** Tablet-Server-Adresse + Felder-Übersicht (Rust: commands::TabletInfo). */
export interface TabletInfo {
  server_host: string;
  /** "lan" oder "cloud". */
  mode: ConnectionMode;
  /** Im Cloud-Modus die öffentliche Relay-Basis-URL, sonst leer. */
  relay_base: string;
  courts: CourtOverview[];
}

/** Ein kampflos wertbares Spiel (Rust: tablet::state::WalkoverCandidate). */
export interface WalkoverCandidate {
  match_id: number;
  draw_id: number;
  planning_id: number;
  /** Runden-/Spielbezeichnung, z. B. "G3". */
  round_name: string;
  /** Anzeigename des Gegners, der den kampflosen Sieg erhielte. */
  opponent: string;
  /** Steht die aufgebende Mannschaft auf Seite 1 des Matches? */
  retired_is_team1: boolean;
}

/** Walkover-Vorschlag nach einer Aufgabe (Rust: commands::WalkoverProposalView). */
export interface WalkoverProposal {
  id: string;
  /** Anzeigename der aufgebenden Mannschaft. */
  retired_team: string;
  /** Disziplin/Auslosung der Aufgabe, z. B. "HE". */
  draw_name: string;
  created_at_ms: number;
  candidates: WalkoverCandidate[];
}

/** Ergebnis einer Walkover-Bestätigung (Rust: commands::WalkoverResult). */
export interface WalkoverResult {
  /** Anzahl erfolgreich nach BTP geschriebener kampfloser Wertungen. */
  written: number;
  /** Fehlermeldungen der nicht geschriebenen Spiele. */
  errors: string[];
}
