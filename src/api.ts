// Typsichere Wrapper um die Tauri-Commands (src-tauri/src/commands.rs).

import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  CloudAnnounce,
  CourtAd,
  DrawInfo,
  FinishedMatchRow,
  FreetextItem,
  MonitorDeviceInfo,
  MonitorTarget,
  NameOverride,
  PairingCode,
  PreparationView,
  InternetStatus,
  SlaveInfo,
  SlaveDeviceInfo,
  SyncStatus,
  TabletInfo,
  TournamentStats,
  WalkoverProposal,
  WalkoverResult,
  WifiStatus,
  WinnersView,
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

/** Synthetisiert eine Ansage per Azure Neural TTS; liefert MP3 als Base64.
 *  Wirft, wenn Azure aus/fehlerhaft ist → Aufrufer fällt auf Web Speech zurück. */
export const azureTtsSpeak = (ssml: string): Promise<string> =>
  invoke("azure_tts_speak", { ssml });

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

/**
 * Ergebnis eines Spiels aus der Turnierleitung eintragen (Backend-
 * Finalisierung): `sets` als [[team1, team2], …]. Reguläres Ergebnis;
 * serverseitig R5-validiert. Steht das Spiel noch auf einem Feld, wird es
 * im selben Zug freigegeben.
 */
export const enterResult = (
  matchId: number,
  sets: [number, number][],
): Promise<void> => invoke("enter_result", { matchId, sets });

/** Verwirft einen Walkover-Vorschlag, ohne ihn umzusetzen. */
export const dismissWalkover = (proposalId: string): Promise<void> =>
  invoke("dismiss_walkover", { proposalId });

/** Ruf-bare Spiele + Hallen des Turniers für den „In Vorbereitung"-Tab. */
export const preparationCandidates = (): Promise<PreparationView> =>
  invoke("preparation_candidates");

/** Abgeschlossene Spiele (mit Sieger) für die Spielübersicht-Tabelle. */
export const finishedMatches = (): Promise<FinishedMatchRow[]> =>
  invoke("finished_matches");

/** Auslosungen (Disziplin + draw_name) des Turniers — für die Disziplin→Halle-Einstellung. */
export const tournamentDraws = (): Promise<DrawInfo[]> =>
  invoke("tournament_draws");

/** Turnier-Kennzahlen fürs Dashboard (null ohne Snapshot). */
export const tournamentStats = (): Promise<TournamentStats | null> =>
  invoke("tournament_stats");

/** Cloud-Ansage-Slave: Hallen-Matches + neue Freitexte aus dem Master-Relay. */
export const cloudAnnounceState = (since: number): Promise<CloudAnnounce> =>
  invoke("cloud_announce_state", { since });

/** Master: ferne Hallen (Cloud-Slaves) samt Online-Status. */
export const cloudSlaves = (): Promise<SlaveInfo[]> => invoke("cloud_slaves");

/** Master: kurzlebigen 8-stelligen Telefon-Kopplungscode erzeugen (ADR 0004).
 *  Braucht laufenden Cloud-Modus; 1 Stunde gültig. */
export const pairingCode = (): Promise<PairingCode> => invoke("pairing_code");

/** Slave: Telefon-Code gegen den vollen Master-Kopplungs-Code einlösen. */
export const resolvePairingCode = (code: string): Promise<string> =>
  invoke("resolve_pairing_code", { code });

/** Slave: Relay-Basis + Felder der eigenen Halle für den Geräte-Anschluss
 *  (Tablet-QR + Monitor-Link je Feld). Leer, wenn kein Cloud-Slave. */
export const slaveDevices = (): Promise<SlaveDeviceInfo> =>
  invoke("slave_devices");

/** Geteiltes Aussprache-Wörterbuch von badhub laden (offline: aus Cache). */
export const fetchPronunciations = (): Promise<NameOverride[]> =>
  invoke("fetch_pronunciations");

/** Eigene Aussprache-Korrekturen mit der Community-DB teilen (opt-in). */
export const sharePronunciations = (entries: NameOverride[]): Promise<number> =>
  invoke("share_pronunciations", { entries });

/** Master: eine Freitext-Ansage ablegen (Halle leer = alle). Liefert die ID. */
export const publishFreetext = (hall: string, text: string): Promise<number> =>
  invoke("publish_freetext", { hall, text });

/** Neue Freitext-Ansagen (id > since) für die eigene Halle (Master: lokal,
 *  Slave: vom Master geholt). */
export const pendingFreetext = (since: number): Promise<FreetextItem[]> =>
  invoke("pending_freetext", { since });

/** Ruft die ausgewählten Spiele „in Vorbereitung" (optional je Halle). */
export const callPreparation = (
  matchIds: number[],
  locationId: number | null,
): Promise<void> =>
  invoke("call_preparation", { matchIds, locationId });

/** Nimmt den „in Vorbereitung"-Aufruf eines Spiels zurück. */
export const retractPreparation = (matchId: number): Promise<void> =>
  invoke("retract_preparation", { matchId });

/** Podien aller ausgespielten Disziplinen + aktuell gewählte Disziplin. */
export const winnersOverview = (): Promise<WinnersView> =>
  invoke("winners_overview");

/** Wählt die auf dem Sieger-Monitor gezeigte Disziplin (null = nichts). */
export const setWinnersSelection = (drawId: number | null): Promise<void> =>
  invoke("set_winners_selection", { drawId });

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

/** Liest eine gewählte Logo-Datei und liefert sie Base64-kodiert + MIME zurück
 *  (zum Ablegen in config.tournament_logo). */
export const readTournamentLogo = (
  path: string,
): Promise<{ data: string; mime: string }> =>
  invoke("read_tournament_logo", { path });

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
