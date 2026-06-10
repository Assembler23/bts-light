// Typsichere Wrapper um die Tauri-Commands (src-tauri/src/commands.rs).

import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  CourtAd,
  MonitorDeviceInfo,
  MonitorTarget,
  PreparationView,
  InternetStatus,
  SyncStatus,
  TabletInfo,
  WalkoverProposal,
  WalkoverResult,
  WifiStatus,
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

/** Aktuelles WLAN (SSID) des Turnier-PCs für die Kopfzeile. */
export const wifiStatus = (): Promise<WifiStatus> => invoke("wifi_status");

/** Internet-/Uplink-Status (badhub-Cloud erreichbar?) für die Kopfzeile. */
export const internetStatus = (): Promise<InternetStatus> =>
  invoke("internet_status");

/** Öffnet die Live-Seite im Browser. display: null | "monitor" | "next". */
export const openLiveView = (display: string | null): Promise<void> =>
  invoke("open_live_view", { display });

/** Tablet-Server-Adresse + Felder-Übersicht für die Turnierleitung. */
export const tabletOverview = (): Promise<TabletInfo> =>
  invoke("tablet_overview");

/** Weist ein Match einem Feld zu (schreibt nach BTP). */
export const assignCourt = (matchId: number, courtId: number): Promise<void> =>
  invoke("assign_court", { matchId, courtId });

/** Gibt ein Feld frei (schreibt nach BTP). */
export const freeCourt = (courtId: number): Promise<void> =>
  invoke("free_court", { courtId });

/** Feld sperren/entsperren (bts-light-seitig, persistiert in Config). */
export const setCourtLocked = (courtId: number, locked: boolean): Promise<void> =>
  invoke("set_court_locked", { courtId, locked });

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

/** Ruf-bare Spiele + Hallen des Turniers für den „In Vorbereitung"-Tab. */
export const preparationCandidates = (): Promise<PreparationView> =>
  invoke("preparation_candidates");

/** Ruft die ausgewählten Spiele „in Vorbereitung" (optional je Halle). */
export const callPreparation = (
  matchIds: number[],
  locationId: number | null,
): Promise<void> =>
  invoke("call_preparation", { matchIds, locationId });

/** Nimmt den „in Vorbereitung"-Aufruf eines Spiels zurück. */
export const retractPreparation = (matchId: number): Promise<void> =>
  invoke("retract_preparation", { matchId });

/** Übernimmt ein gewähltes Werbebild in das Court-Monitor-Verzeichnis. */
export const addCourtAd = (path: string): Promise<string> =>
  invoke("add_court_ad", { path });

/** Entfernt ein Werbebild aus dem Court-Monitor-Verzeichnis. */
export const removeCourtAd = (file: string): Promise<void> =>
  invoke("remove_court_ad", { file });

/** Listet die hinterlegten Court-Monitor-Werbebilder samt optionalem
 *  Anzeige-Label. Ein leeres Label bedeutet "noch kein Name vergeben". */
export const listCourtAds = (): Promise<CourtAd[]> => invoke("list_court_ads");

/** Setzt (oder löscht bei leerem Label) den Anzeigenamen eines Werbebilds. */
export const setCourtAdLabel = (file: string, label: string): Promise<void> =>
  invoke("set_court_ad_label", { file, label });

/** Liefert die Court-Monitor-Geräte für die Verwaltungsseite. */
export const monitorDevices = (): Promise<MonitorDeviceInfo[]> =>
  invoke("monitor_devices");

/** Weist ein Monitor-Gerät einem Target zu — entweder einem Feld oder
 *  einer Hallen-weiten Info-Anzeige. `null` = Zuweisung aufheben. */
export const assignMonitor = (
  deviceId: string,
  target: MonitorTarget | null,
): Promise<void> => invoke("assign_monitor", { deviceId, target });

/** Legt für ein Monitor-Gerät explizit eine Halle fest (Hallenname) oder hebt
 *  sie auf (`null`). Für Geräte ohne Feld (Info/Werbung/Kombi/unzugewiesen). */
export const setMonitorHall = (
  deviceId: string,
  hall: string | null,
): Promise<void> => invoke("set_monitor_hall", { deviceId, hall });

/** Schickt einem Monitor-Gerät einen Fernbefehl. */
export const monitorCommand = (
  deviceId: string,
  kind: "reload" | "identify",
): Promise<void> => invoke("monitor_command", { deviceId, kind });

/** Entfernt ein offline Monitor-Gerät aus der Liste (Online wird vom
 *  Backend abgelehnt). */
export const forgetMonitorDevice = (deviceId: string): Promise<void> =>
  invoke("forget_monitor_device", { deviceId });
