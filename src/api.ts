// Typsichere Wrapper um die Tauri-Commands (src-tauri/src/commands.rs).

import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  SyncStatus,
  TabletInfo,
  WalkoverProposal,
  WalkoverResult,
} from "./types";

export const loadConfig = (): Promise<AppConfig> => invoke("load_config");

export const saveConfig = (config: AppConfig): Promise<void> =>
  invoke("save_config", { config });

/** Testet die BTP-Verbindung, liefert bei Erfolg den Turniernamen. */
export const testBtp = (
  host: string,
  port: number,
  password: string | null,
): Promise<string> => invoke("test_btp", { host, port, password });

export const startSync = (): Promise<void> => invoke("start_sync");

export const stopSync = (): Promise<void> => invoke("stop_sync");

export const getStatus = (): Promise<SyncStatus> => invoke("get_status");

/** Öffnet die Live-Seite im Browser. display: null | "monitor" | "next". */
export const openLiveView = (display: string | null): Promise<void> =>
  invoke("open_live_view", { display });

/** Tablet-Server-Adresse + Felder-Übersicht für die Turnierleitung. */
export const tabletOverview = (): Promise<TabletInfo> =>
  invoke("tablet_overview");

/** Öffnet das Log-Verzeichnis im Datei-Manager. */
export const openLogDir = (): Promise<void> => invoke("open_log_dir");

/** Installierte App-Version (Rust: CARGO_PKG_VERSION). */
export const appVersion = (): Promise<string> => invoke("app_version");

/** Öffnet eine externe https-URL im Standardbrowser. */
export const openExternal = (url: string): Promise<void> =>
  invoke("open_external", { url });

/** Offene Walkover-Vorschläge nach Aufgaben (Aufgabe → restliche Spiele). */
export const walkoverProposals = (): Promise<WalkoverProposal[]> =>
  invoke("walkover_proposals");

/** Wertet die ausgewählten Spiele kampflos (Walkover) nach BTP. */
export const confirmWalkover = (
  proposalId: string,
  matchIds: number[],
): Promise<WalkoverResult> =>
  invoke("confirm_walkover", { proposalId, matchIds });

/** Verwirft einen Walkover-Vorschlag, ohne ihn umzusetzen. */
export const dismissWalkover = (proposalId: string): Promise<void> =>
  invoke("dismiss_walkover", { proposalId });
