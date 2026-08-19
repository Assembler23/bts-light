// Typsichere Wrapper um die Tauri-Commands (src-tauri/src/commands.rs).

import { invoke } from "@tauri-apps/api/core";
import type {
  AnnounceJob,
  AppConfig,
  CheckinView,
  CloudAnnounce,
  CourtAd,
  DrawInfo,
  FinishedMatchRow,
  MatchTimeline,
  FreetextItem,
  HallColorsView,
  HallLayoutConfig,
  MonitorDeviceInfo,
  MonitorTarget,
  NameOverride,
  PairingCode,
  PreparationView,
  ScorekeeperEntry,
  OfficialView,
  AppearanceView,
  BlocklistView,
  CourtSwitchesView,
  InternetStatus,
  SlaveInfo,
  SlaveDeviceInfo,
  SyncStatus,
  TabletInfo,
  TlPairing,
  TlWebInfo,
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

/** Sichert den aufgelaufenen Live-Stand sofort auf die Platte.
 *
 *  Nötig vor jedem Beenden, das nicht über `stopSync` oder das Schließen des
 *  Fensters läuft — heute der Neustart nach einem Auto-Update. Seit der
 *  Entprellung (Spec `monitor-livestand-push`, S2) schreibt nicht mehr jeder
 *  gezählte Punkt selbst. */
export const flushLiveScores = (): Promise<void> => invoke("flush_live_scores");

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
export const setCourtLocked = (
  courtId: number,
  locked: boolean,
): Promise<void> => invoke("set_court_locked", { courtId, locked });

/** Spiel von der automatischen Feldvergabe ausnehmen/wieder aufnehmen (Spec
 *  `feldvergabe-ausnahme`) — betrifft nie manuelles Zuweisen. */
export const autoAssignExclude = (
  matchId: number,
  excluded: boolean,
): Promise<void> => invoke("auto_assign_exclude", { matchId, excluded });

/** Ein noch nicht gerufenes Spiel vor ein anderes ziehen (Spec
 *  `spielliste-manuelle-reihenfolge`, ADR 0023) — die Halle wird
 *  serverseitig aus dem Match abgeleitet. `beforeMatchId = null` heißt
 *  „ans Ende des aktuell sichtbaren Präfix-Blocks". */
export const queueReorder = (
  matchId: number,
  beforeMatchId: number | null,
): Promise<void> => invoke("queue_reorder", { matchId, beforeMatchId });

/** Verwirft die manuelle Spielreihenfolge ALLER Hallen auf einmal (globaler
 *  Reset-Knopf, Spec `spielliste-manuelle-reihenfolge`). */
export const queueOrderReset = (): Promise<void> => invoke("queue_order_reset");

/** Öffnet das Log-Verzeichnis im Datei-Manager. */
export const openLogDir = (): Promise<void> => invoke("open_log_dir");

/** Master-Identität exportieren (ADR 0006): JSON-Bündel (install_id +
 *  Einstellungen, OHNE Passwörter) für den Umzug auf einen neuen Turnier-PC. */
export const exportIdentity = (): Promise<string> => invoke("export_identity");

/** Master-Identität importieren (ADR 0006): übernimmt install_id + Einstellungen
 *  aus dem Bündel; die lokal gesetzten Passwörter bleiben. Liefert die neue Config. */
export const importIdentity = (bundle: string): Promise<AppConfig> =>
  invoke("import_identity", { bundle });

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

/**
 * Disqualifikation aus der Turnierleitung (P3, BTP-ScoreStatus 3): `loserTeam`
 * (1 oder 2) wird disqualifiziert, der Gegner gewinnt. Bereits gespielte `sets`
 * bleiben erhalten (Disqualifikation kann mitten im Spiel fallen).
 */
export const disqualifyMatch = (
  matchId: number,
  loserTeam: 1 | 2,
  sets: [number, number][],
): Promise<void> => invoke("disqualify_match", { matchId, loserTeam, sets });

/** Verwirft einen Walkover-Vorschlag, ohne ihn umzusetzen. */
export const dismissWalkover = (proposalId: string): Promise<void> =>
  invoke("dismiss_walkover", { proposalId });

/** Ruf-bare Spiele + Hallen des Turniers für den „In Vorbereitung"-Tab. */
export const preparationCandidates = (): Promise<PreparationView> =>
  invoke("preparation_candidates");

/** Abgeschlossene Spiele (mit Sieger) für die Spielübersicht-Tabelle. */
export const finishedMatches = (): Promise<FinishedMatchRow[]> =>
  invoke("finished_matches");

/** Punktverlauf eines Matches (null = kein Verlauf aufgezeichnet). */
export const matchTimeline = (matchId: number): Promise<MatchTimeline | null> =>
  invoke("match_timeline", { matchId });

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

/** Check-In-Stand aus badhub holen (Hallen-Check-In, Schnitt C).
 *
 *  Wirft **nie**: fehlendes Internet und ein badhub ohne den Check-In-Kanal
 *  kommen als `availability` zurück, damit die Seite einen Hinweis zeigen kann
 *  statt einer Fehlermeldung (AK-C3, C4). */
export const checkinState = (): Promise<CheckinView> => invoke("checkin_state");

/** Einen Spieler von Hand setzen (`check_in`), zurücksetzen (`reset`, sperrt
 *  den Selbst-Check-In) oder entsperren (`unlock`). */
export const checkinSetPlayer = (
  eventId: number,
  playerId: number,
  action: "check_in" | "reset" | "unlock",
): Promise<void> =>
  invoke("checkin_set_player", { eventId, playerId, action });

/** Anfangszeit und Anmeldeschluss einer Klasse ändern. `null` löscht den Wert.
 *  Der Wert landet sofort in badhub — bts-light hält keine eigene Kopie. */
export const checkinSetTimes = (
  eventId: number,
  startsAt: string | null,
  closesAt: string | null,
): Promise<void> => invoke("checkin_set_times", { eventId, startsAt, closesAt });

/** Ansagetext für eine Klasse bauen: `deadline` („Noch N Minuten bis
 *  Anmeldeschluss …") oder `missing` (die Fehlenden). `null` heißt: es gibt
 *  nichts anzusagen. Der Stand wird dafür frisch geholt. */
export const checkinAnnouncement = (
  eventId: number,
  kind: "deadline" | "missing",
): Promise<string | null> =>
  invoke("checkin_announcement", { eventId, kind });

/** Master: eine Freitext-Ansage ablegen (Halle leer = alle). Liefert die ID. */
export const publishFreetext = (hall: string, text: string): Promise<number> =>
  invoke("publish_freetext", { hall, text });

/** Neue Freitext-Ansagen (id > since) für die eigene Halle (Master: lokal,
 *  Slave: vom Master geholt). */
export const pendingFreetext = (since: number): Promise<FreetextItem[]> =>
  invoke("pending_freetext", { since });

/** Neue Ansage-Aufträge der Turnierleitung (id > since) für die eigene Halle.
 *  Die Seite selbst spricht nie — sie beauftragt, gesprochen wird hier. */
export const pendingAnnounceJobs = (since: number): Promise<AnnounceJob[]> =>
  invoke("pending_announce_jobs", { since });

/** Meldet dem Turnier-PC, welche Aufruf-Stufe gerade angesagt wurde — damit
 *  Desktop und Turnierleitungs-Seite dieselbe Zahl führen. Gemeldet wird die
 *  **gesprochene** Stufe, nicht „noch einmal": Die Oberfläche weiß genau, was
 *  sie gesagt hat. */
export const noteCourtCall = (
  courtId: number,
  matchId: number,
  stage: number,
): Promise<number> => invoke("note_court_call", { courtId, matchId, stage });

/** Zustand der Turnierleitungs-Oberfläche samt gekoppelter Geräte. */
export const tlWebInfo = (): Promise<TlWebInfo> => invoke("tl_web_info");

/** Koppelt ein Gerät und liefert die Zugänge **plus die neue
 *  Konfiguration**. Letztere muss die App übernehmen: Bliebe ihre Kopie
 *  veraltet, schickte der nächste Speichervorgang aus den Einstellungen den
 *  alten `tl_web`-Stand zurück — und löschte alle Kopplungen.
 *
 *  Kennung und Zugang erzeugt die Oberfläche selbst — derselbe Weg wie bei
 *  der `install_id`. */
export const tlDeviceAdd = (
  label: string,
  hall: string,
): Promise<[TlPairing, AppConfig]> =>
  invoke("tl_device_add", {
    id: `tl-${crypto.randomUUID().slice(0, 8)}`,
    token: crypto.randomUUID(),
    label,
    hall,
  });

/** Entzieht einem Gerät den Zugang; liefert die neue Konfiguration. */
export const tlDeviceRemove = (id: string): Promise<AppConfig> =>
  invoke("tl_device_remove", { id });

/** Schaltet die Oberfläche an oder ab (Geräte bleiben); liefert die neue
 *  Konfiguration. */
export const tlWebSetEnabled = (enabled: boolean): Promise<AppConfig> =>
  invoke("tl_web_set_enabled", { enabled });

/** Legt die Raster-Anordnung einer Halle fest (oder ersetzt sie); liefert
 *  die neue Konfiguration. */
export const setHallLayout = (layout: HallLayoutConfig): Promise<AppConfig> =>
  invoke("set_hall_layout", { layout });

/** Entfernt die Anordnung einer Halle — zurück zur Fließ-Darstellung;
 *  liefert die neue Konfiguration. */
export const removeHallLayout = (hall: string): Promise<AppConfig> =>
  invoke("remove_hall_layout", { hall });

/** Übersteuert die Farbe einer Halle (Palettenton, Spec hallen-farben);
 *  liefert die neue Konfiguration. */
export const setHallColor = (
  hall: string,
  color: string,
): Promise<AppConfig> => invoke("set_hall_color", { hall, color });

/** Entfernt die Farb-Übersteuerung einer Halle — zurück zur Auto-Palette;
 *  liefert die neue Konfiguration. */
export const removeHallColor = (hall: string): Promise<AppConfig> =>
  invoke("remove_hall_color", { hall });

/** Palette + effektive Farbe je Halle für den Picker der Felderübersicht. */
export const hallColorsView = (): Promise<HallColorsView> =>
  invoke("hall_colors_view");

/** Ruft die ausgewählten Spiele „in Vorbereitung" (optional je Halle). */
export const callPreparation = (
  matchIds: number[],
  locationId: number | null,
): Promise<void> => invoke("call_preparation", { matchIds, locationId });

/** Nimmt den „in Vorbereitung"-Aufruf eines Spiels zurück. */
export const retractPreparation = (matchId: number): Promise<void> =>
  invoke("retract_preparation", { matchId });

/** Zähltafelbediener-Warteschlange (FIFO) — Verlierer regulär beendeter Spiele
 *  (ADR 0007). */
export const scorekeeperQueue = (): Promise<ScorekeeperEntry[]> =>
  invoke("scorekeeper_queue");

/** Einen Wartenden aus der Zähltafelbediener-Schlange entfernen. */
export const removeScorekeeper = (key: string): Promise<void> =>
  invoke("remove_scorekeeper", { key });

/** Einen Wartenden an den Anfang der Schlange ziehen (als Nächsten dran). */
export const advanceScorekeeper = (key: string): Promise<void> =>
  invoke("advance_scorekeeper", { key });

/** Manuell einen Zähltafelbediener hinzufügen. */
export const addScorekeeper = (names: string[]): Promise<void> =>
  invoke("add_scorekeeper", { names });

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

/** Markiert ein Werbebild als „auch klein in der Leiste zeigen" (oder entfernt
 *  die Markierung). Die Leiste zeigt genau die markierten Bilder. */
export const setCourtAdBar = (file: string, inBar: boolean): Promise<void> =>
  invoke("set_court_ad_bar", { file, inBar });

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

// ───────────── Schiedsrichter (Spec schiedsrichter-management) ─────────────

/** Die Schiedsrichterliste des Turniers in Rotationsreihenfolge, angereichert
 *  um Pausen, Stammverein, aktuellen Dienst und Einsatz-Zähler. */
export const officialsRoster = (): Promise<OfficialView[]> =>
  invoke("officials_roster");

/** Weist einem Spiel einen Schiedsrichter (`"sr"`) oder Aufschlagrichter
 *  (`"ar"`) zu. Liefert die Konflikt-Kategorie, falls einer besteht — die
 *  Zuweisung wird trotzdem ausgeführt (die Turnierleitung entscheidet). */
export const officialAssign = (
  matchId: number,
  role: "sr" | "ar",
  officialId: number,
): Promise<string | null> =>
  invoke("official_assign", { matchId, role, officialId });

/** Löst die Zuweisung eines Spiels. */
export const officialClear = (
  matchId: number,
  role: "sr" | "ar",
): Promise<void> => invoke("official_clear", { matchId, role });

/** Pausiert einen Schiedsrichter oder aktiviert ihn wieder. */
export const officialPause = (
  officialId: number,
  paused: boolean,
): Promise<void> => invoke("official_pause", { officialId, paused });

/** Zieht einen Schiedsrichter in der Reihenfolge vor einen anderen
 *  (`beforeOfficialId = null` ⇒ ans Ende). */
export const officialReorder = (
  officialId: number,
  beforeOfficialId: number | null,
): Promise<void> =>
  invoke("official_reorder", { officialId, beforeOfficialId });

/** Pflegt den Stammverein (BTP liefert am Official keinen). */
export const officialSetClub = (
  officialId: number,
  club: string,
): Promise<void> => invoke("official_set_club", { officialId, club });

/** Lädt die Sperrlisten eines Schiedsrichters — gezielt auf Anfrage, damit
 *  diese Personendaten nicht in jeder Listen-Abfrage mitreisen. */
export const officialBlocklists = (
  officialId: number,
): Promise<BlocklistView> => invoke("official_blocklists", { officialId });

/** Setzt die Sperrlisten eines Schiedsrichters (ersetzt beide Listen). */
export const officialSetBlocklists = (
  officialId: number,
  clubs: string[],
  players: number[],
): Promise<void> =>
  invoke("official_set_blocklists", { officialId, clubs, players });

/** Die Einsätze eines Schiedsrichters im Detail (Spiel, Rolle, Feld, Ende). */
export const officialAppearances = (
  officialId: number,
): Promise<AppearanceView[]> => invoke("official_appearances", { officialId });

/** Feldweise Schalter (SR-Rotation, AR-Rotation, Bediener-Vergabe). */
export const officialsCourtSwitches = (): Promise<CourtSwitchesView[]> =>
  invoke("officials_court_switches");

/** Setzt die feldweisen Schalter eines Felds. */
export const officialsSetCourtSwitches = (
  courtId: number,
  sr: boolean,
  ar: boolean,
  operator: boolean,
): Promise<void> =>
  invoke("officials_set_court_switches", { courtId, sr, ar, operator });
