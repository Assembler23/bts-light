// TypeScript-Spiegel der Rust-Strukturen (src-tauri/src/config.rs, commands.rs).

export interface BtpConfig {
  host: string;
  port: number;
  password: string | null;
}

export interface BadhubConfig {
  url: string;
  password: string;
}

export interface AppConfig {
  btp: BtpConfig;
  badhub: BadhubConfig;
}

export interface SyncStatus {
  running: boolean;
  /** "idle" | "ok" | "btp_error" | "push_error" */
  kind: string;
  message: string;
  updated_at_ms: number;
}
