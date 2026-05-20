// Typsichere Wrapper um die Tauri-Commands (src-tauri/src/commands.rs).

import { invoke } from "@tauri-apps/api/core";
import type { AppConfig, SyncStatus } from "./types";

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
