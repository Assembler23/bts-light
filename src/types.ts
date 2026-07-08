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

/** Verbindungsart für die Schiedsrichter-Tablets. `"lan+cloud"` betreibt
 *  beide Wege gleichzeitig (z. B. Zwei-Hallen-Turnier). */
export type ConnectionMode = "lan" | "cloud" | "lan+cloud";

/** Sprachmodus der Feld-Ansagen (Rust: config::AnnounceLanguageMode). */
export type AnnounceLanguageMode = "de" | "en" | "auto";

/** Einstellungen der gesprochenen Feld-Ansagen (Rust: config::AnnounceConfig). */
export interface AnnounceConfig {
  /** Ansagen aktiv? */
  enabled: boolean;
  /** Deutsch / Englisch / Automatisch. */
  language_mode: AnnounceLanguageMode;
  /** Bevorzugte deutsche Stimme (voiceURI); leer = Browser-Standard. */
  voice_de: string;
  /** Bevorzugte englische Stimme (voiceURI); leer = Browser-Standard. */
  voice_en: string;
  /** Sprech-Geschwindigkeit (0,5–1,5). */
  rate: number;
  /** Gong vor der Ansage abspielen? */
  gong: boolean;
  /** Phonetische Aussprache-Korrekturen (Name/Namensteil → gesprochene Form). */
  name_overrides: NameOverride[];
  /** Aussprache-Korrekturen anwenden (Basis-Wörterbuch + Einträge)? Default an. */
  name_overrides_enabled: boolean;
  /** Mehr-Hallen-Turnier: nur Spiele dieser Halle (BTP-Location-Name) ansagen.
   *  Leer = alle Hallen (Standard). So hört jede Halle nur ihre eigenen Ansagen. */
  announce_hall: string;
  /** Gespeicherte Ansage-Blöcke für wiederkehrende Freitext-Ansagen. */
  saved_announcements: string[];
  /** Opt-in: eigene Aussprache-Korrekturen mit der Community-DB teilen. */
  share_corrections: boolean;
}

/** Azure Neural TTS für die Ansage (Rust: config::AzureTtsConfig). */
export interface AzureTtsConfig {
  /** Azure-TTS für die Ansage verwenden? */
  enabled: boolean;
  /** Azure-Region der Speech-Ressource, z. B. „westeurope". */
  region: string;
  /** Subscription-Key (KEY 1) der Speech-Ressource. */
  key: string;
  /** Mehrsprachige Stimme, z. B. „de-DE-SeraphinaMultilingualNeural". */
  voice: string;
}

/** Aussprache-Korrektur für die Ansage (Rust: config::NameOverride). */
export interface NameOverride {
  /** Ganzer Name ODER einzelner Namensteil (z. B. Nachname). */
  name: string;
  /** Phonetische Ersatz-Schreibweise, z. B. „Nguyen" → „Nwujen". */
  say: string;
  /** Optionales IPA-Phoneme (W3C PLS) für den Azure-Pfad (`<phoneme>`).
   *  Kommt aus dem geteilten Wörterbuch (Lexikon); im Web-Speech-Pfad ungenutzt. */
  ipa?: string;
  /** Optionale manuelle Sprach-Korrektur für den Azure-`<lang>`-Pfad. Leer =
   *  automatisch; `"de"` = erzwingt deutschen Default; sonst NameLang. */
  lang?: string;
}

/** Einstellungen des Aufruf-Timers (Rust: config::CallTimerConfig). */
export interface CallTimerConfig {
  /** Aufruf-Timer aktiv? */
  enabled: boolean;
  /** Minuten nach dem 1. Aufruf, ab denen der 2. Aufruf fällig ist. */
  second_call_minutes: number;
  /** Minuten nach dem 1. Aufruf, ab denen der 3./letzte Aufruf fällig ist. */
  third_call_minutes: number;
}

/** Einstellungen der automatischen Feldvergabe (Rust: config::AutoAssignConfig). */
export interface AutoAssignConfig {
  /** Automatische Feldvergabe aktiv? */
  enabled: boolean;
  /** Wartezeit in Minuten, die ein Feld frei sein muss, bevor automatisch belegt wird. */
  wait_minutes: number;
  /** Mindest-Pause eines Spielers nach seinem letzten Spiel (Minuten), bevor er
   *  wieder aufgerufen wird. 0 = Wert aus BTP (Setting 1303) übernehmen. */
  pause_minutes: number;
  /** Aktive Halle (BTP-Location-Name) für Mehr-Hallen-Turniere mit nur einer
   *  bespielten Halle pro Tag. Gesetzt = Auto-Vergabe nur dort, ohne manuellen
   *  „in Vorbereitung"-Aufruf. Leer = alle Hallen. */
  active_hall: string;
}

/** Einstellungen der Court-Monitor-Anzeige (Rust: config::CourtMonitorConfig). */
export interface CourtMonitorConfig {
  /** Court-Monitor eingerichtet/aktiv? Steuert nur die Sichtbarkeit der
   *  Monitor-Adressen – die Anzeige-Seite ist immer erreichbar. */
  enabled: boolean;
  /** Wechsel-Intervall der Werbebilder im Leerlauf (Sekunden). */
  ad_interval_s: number;
  /** Disziplin in der Kopfzeile anzeigen? */
  show_discipline: boolean;
  /** Runde in der Fußzeile anzeigen? */
  show_round: boolean;
  /** Spielnummer in der Fußzeile anzeigen? */
  show_match_number: boolean;
  /** Pausen-Countdown (Retro-Klappanzeige) anzeigen? */
  show_timer: boolean;
  /** Spieldauer (Minuten, mit Stoppuhr-Symbol) in der Kopfzeile anzeigen? */
  show_match_clock: boolean;
  /** Werbung im Leerlauf anzeigen? Aus → neutrale Leerlauf-Seite. */
  show_ads: boolean;
  /** Anzeige-Layout des Monitors (`split` = „A — Geteilt"). */
  layout: string;
  /** Kombi-Anzeige: Felder nebeneinander (Hochformat) statt übereinander. */
  combo_vertical: boolean;
}

/** Ein Werbebild mit optionalem Anzeige-Label (Rust: commands::CourtAd).
 *  `label === ""` bedeutet "noch kein Anzeigename gesetzt" – die UI
 *  rendert dann den Dateinamen als Fallback. */
export interface CourtAd {
  file: string;
  label: string;
}

/** Was ein Court-Monitor-Gerät anzeigen soll – Feld, Info-Anzeige oder
 *  Werbung (Rust: relay_proto::MonitorTarget). */
export type MonitorTarget =
  | { kind: "court"; court_id: number }
  | { kind: "info_overview"; hall?: string | null }
  | { kind: "info_preparation" }
  | { kind: "info_winners"; rank?: number | null }
  | { kind: "ad_rotation" }
  | { kind: "ad_single"; file: string }
  | { kind: "court_combo"; court_ids: number[] };

/** Ein platzierter Spieler im Podium (Rust: tablet::winners::WinnerPlayer). */
export interface WinnerPlayer {
  name: string;
  /** Vorname(n) und Nachname getrennt — fürs zweizeilige Rendern im Sieger-Monitor. */
  first: string;
  last: string;
  club: string | null;
}

/** Eine Platzierung im Podium (Rust: tablet::winners::Placement). */
export interface Placement {
  rank: number;
  players: WinnerPlayer[];
  walkover: boolean;
}

/** Podium einer ausgespielten Disziplin (Rust: tablet::winners::DisciplineResult). */
export interface DisciplineResult {
  draw_id: number;
  draw_name: string;
  discipline: string;
  podium: Placement[];
  finished_at: number | null;
}

/** Steuer-Ansicht der Siegerehrung (Rust: commands::WinnersView). */
export interface WinnersView {
  disciplines: DisciplineResult[];
  /** Draw-ID der aktuell gezeigten Disziplin, oder null. */
  selected: number | null;
}

/** Ein Court-Monitor-Gerät (Rust: relay_proto::MonitorDeviceInfo). */
export interface MonitorDeviceInfo {
  /** Stabile Geräte-ID (UUID, vom Monitor selbst erzeugt). */
  id: string;
  /** Kurz-Code, wie ihn der TV anzeigt. */
  code: string;
  /** CourtID des zugewiesenen Felds (Identität), oder null bei
   *  unzugewiesen ODER Info-Target (dann steht der Typ in `target`). */
  courtId: number | null;
  /** Feldname (Anzeige) des zugewiesenen Felds, oder null. */
  court: string | null;
  /** Vollständige Zuweisung (Feld oder Info-Display) – null wenn das
   *  Gerät noch keinem Target zugewiesen ist. */
  target: MonitorTarget | null;
  /** Hat sich das Gerät zuletzt gemeldet? */
  online: boolean;
  /** Vom Operator explizit gewählte Halle (Hallenname); überschreibt die aus
   *  dem Feld abgeleitete Halle. null = keine explizite Wahl. */
  hall: string | null;
}

/** Eine Disziplin/Klasse→Halle-Regel (Rust: config::DisciplineHallRule).
 *  `draw_name` leer = Kategorie-Default (alle Auslosungen der `discipline`);
 *  gesetzt = Override für genau diese Auslosung (z. B. „HE A"). `discipline` =
 *  snake_case-Schlüssel (`mens_singles` …). `hall` = BTP-Location-Name. */
export interface DisciplineHallRule {
  discipline: string;
  draw_name: string;
  hall: string;
}

/** Eine Auslosung/Klasse des Turniers (Rust: commands::DrawInfo). */
export interface DrawInfo {
  discipline: string;
  draw_name: string;
}

/** Präsenz einer fernen Halle / Cloud-Slave (Rust: relay_proto::SlaveInfo). */
export interface SlaveInfo {
  id: string;
  hall: string;
  online: boolean;
  lastSeenMs: number;
}

/** Ein Feld der Cloud-Feldliste (Rust: relay_proto::CourtBrief). */
export interface CourtBrief {
  id: number;
  label: string;
  hall: string;
}

/** Geräte-Anschluss der fernen Halle (Rust: commands::SlaveDeviceInfo).
 *  `relay_base` = `https://badhub.de/bts-relay/<master_ns>`; je Feld baut die
 *  UI daraus Tablet-QR (`<relay_base>/qr/<id>`) und Monitor-Link
 *  (`<relay_base>/court/<id>/display`). */
export interface SlaveDeviceInfo {
  relay_base: string;
  hall: string;
  courts: CourtBrief[];
}

/** Ein Feld im Cloud-Ansage-Status (Rust: commands::CloudAnnounceCourt). */
export interface CloudAnnounceCourt {
  court_id: number;
  court: string;
  discipline: string;
  team1: string[];
  team2: string[];
  team1_nationalities: string[];
  team2_nationalities: string[];
  match_id: number;
}

/** Cloud-Ansage-Status für den fernen Slave (Rust: commands::CloudAnnounce). */
export interface CloudAnnounce {
  courts: CloudAnnounceCourt[];
  freetext: { id: number; hall: string; text: string }[];
}

/** Turnier-Kennzahlen fürs Dashboard (Rust: commands::TournamentStats).
 *  `null`, solange kein Snapshot vorliegt (Liveticker nicht gestartet). */
export interface TournamentStats {
  tournament_name: string;
  n_disciplines: number;
  n_players: number;
  matches_total: number;
  matches_finished: number;
  matches_running: number;
  n_courts: number;
  halls: string[];
}

/** Eine manuelle Freitext-Ansage (Rust: tablet::state::FreetextItem).
 *  `hall` = Ziel-Halle (leer = alle Hallen); `id` fortlaufend (Dedup). */
export interface FreetextItem {
  id: number;
  hall: string;
  text: string;
}

export interface AppConfig {
  btp: BtpConfig;
  badhub: BadhubConfig;
  /** Opt-in: Diagnose-Logs automatisch an badhub.de hochladen. */
  upload_logs: boolean;
  /** Zufällige, dauerhafte Installations-ID (Frontend erzeugt sie). */
  install_id: string;
  /** Verbindungsart für die Tablets: LAN (lokal) oder Cloud (über badhub.de). */
  connection_mode: ConnectionMode;
  /** Ansage-Slave-Modus (Mehr-Hallen): nur BTP lesen + eigene Halle ansagen,
   *  kein Liveticker-Push/Auto-Vergabe/Tablet-Server. Zweiter Rechner in der
   *  anderen Halle, der nur Ansagen macht (es gibt genau einen Master). */
  slave_mode: boolean;
  /** Cloud-Ansage-Slave (Mehr-Hallen über Cloud): Kopplungs-Code/Namespace des
   *  Masters. Gesetzt + slave_mode → Hallen-Matches/Freitext kommen aus dem
   *  Cloud-Relay statt BTP. Leer = klassischer LAN-Slave. */
  master_namespace: string;
  /** Einstellungen der gesprochenen Feld-Ansagen. */
  announce: AnnounceConfig;
  /** Hochwertige Cloud-Ansage über Azure Neural TTS (opt-in). */
  azure_tts: AzureTtsConfig;
  /** Einstellungen der Court-Monitor-Anzeige (TV am Spielfeld). */
  court_monitor: CourtMonitorConfig;
  /** Einstellungen des Aufruf-Timers (1./2./3. Aufruf). */
  call_timer: CallTimerConfig;
  /** Einstellungen der automatischen Feldvergabe. */
  auto_assign: AutoAssignConfig;
  /** Disziplin/Klasse→Halle-Regeln (Mehr-Hallen): schränken die Feldvergabe
   *  ein (manuell + automatisch). Leer = keine Einschränkung. */
  discipline_hall_rules: DisciplineHallRule[];
  /** Vom Operator gesperrte Felder (CourtIDs) – keine Auto-Vergabe. */
  locked_courts: number[];
  /** PIN fürs Einstellungs-Menü am Zähltablett (Feldwechsel ohne QR).
   *  Nur Ziffern, Default „0000". Reiner Bedien-Schutz, keine Sicherheitsgrenze. */
  tablet_settings_pin: string;
  /** Turnierlogo für den badhub-Liveticker (#live-logo). Upload, da BTP keins
   *  liefert. Leere `data` = kein Logo. */
  tournament_logo: LogoConfig;
}

/** Turnierlogo (Base64) für badhubs #live-logo. */
export interface LogoConfig {
  /** Base64-Bilddaten ohne `data:`-Präfix. Leer = kein Logo. */
  data: string;
  /** MIME-Typ, z. B. "image/png". */
  mime: string;
  /** CSS-Hintergrundfarbe hinter dem Logo (leer = badhub-Standard). */
  background_color: string;
}

export interface SyncStatus {
  running: boolean;
  /** "idle" | "ok" | "btp_error" | "push_error" */
  kind: string;
  message: string;
  updated_at_ms: number;
}

/** Internet-/Uplink-Status des Turnier-PCs (Rust: commands::InternetStatus). */
export interface InternetStatus {
  /** Ist die badhub-Cloud erreichbar (Internet/LTE aktiv)? */
  online: boolean;
}

/** Lokaler Netzwerk-Status des Turnier-PCs (Rust: commands::WifiStatus). */
export interface WifiStatus {
  /** Im lokalen BTS-Netz (btsaccess-WLAN oder 192.168.16.x am LAN)? */
  bts_network: boolean;
  /** SSID des verbundenen WLAN; null = kein WLAN (z. B. LAN-Kabel). */
  ssid: string | null;
}

/** Disziplin eines Matches (Rust: btp::model::Discipline). */
export type Discipline =
  | "mens_singles"
  | "womens_singles"
  | "mens_doubles"
  | "womens_doubles"
  | "mixed"
  | "unknown";

/** Eine Court-Zeile der Felder-Übersicht (Rust: tablet::state::CourtOverview). */
export interface CourtOverview {
  /** Stabile BTP-CourtID des Felds – die Identität (Feldnamen wiederholen
   *  sich bei Mehr-Hallen-Turnieren, die CourtID nicht). */
  court_id: number;
  /** Feldname (Anzeige), z. B. „1" oder „Feld 3". */
  court: string;
  /** Hallenname (BTP-Location) des Felds – Grundlage der hallenweisen
   *  Gruppierung. Leer bei Ein-Hallen-Turnieren. */
  location: string;
  /** BTP-Match-ID des aktuellen Spiels (0 = kein Match). */
  match_id: number;
  /** Anzeigename des Matches, z. B. "HE G1"; leer wenn kein Match. */
  match_name: string;
  /** Reine BTP-Runde (z. B. "VF", "HF", "Finale", "Spiel um Platz 3"); leer
   *  wenn keine. Grundlage der K.-o.-Runden-Ansage (ab Viertelfinale). */
  round_name: string;
  /** Disziplin des aktuellen Matches. */
  discipline: Discipline;
  team1: string[];
  team2: string[];
  /** Nationalitäten parallel zu team1 (leerer String = unbekannt). */
  team1_nationalities: string[];
  team2_nationalities: string[];
  /** Satzstand als [Team1, Team2]-Paare. */
  sets: [number, number][];
  tablet_connected: boolean;
  /** Akkustand des Tablets (Android/Chrome) – null bei iPads/Safari. */
  battery: { percent: number; charging: boolean } | null;
  /** Verletzung/Behandlung läuft an diesem Court. */
  injury: boolean;
  /** Die Turnierleitung wurde an diesen Court gerufen. */
  official_call: boolean;
  /** Aufschlagendes Team (1/2) bzw. Spieler-Index (0/1) – Monitor-Anzeige. */
  serving_team: number | null;
  serving_player: number | null;
  /** Voraussichtlicher Zähltafelbediener (Verlierer-Team des Vorspiels auf
   *  diesem Feld). Leer, wenn es kein Vorspiel gab. */
  scorekeeper: string[];
  /** Feld vom Operator gesperrt (bts-light-seitig) → rot, keine Auto-Vergabe. */
  locked: boolean;
  /** Zeitpunkt (Unix-ms) des 1. Aufrufs = seit wann das Spiel auf dem Feld
   *  steht; null = kein Spiel. Grundlage des Aufruf-Timers. */
  on_court_since_ms: number | null;
}

/** Tablet-Server-Adresse + Felder-Übersicht (Rust: commands::TabletInfo). */
export interface TabletInfo {
  /** LAN-Adresse des Tablet-Servers; leer, wenn der LAN-Pfad inaktiv ist. */
  server_host: string;
  /** "lan", "cloud" oder "lan+cloud". */
  mode: ConnectionMode;
  /** Öffentliche Relay-Basis-URL; leer, wenn der Cloud-Pfad inaktiv ist. */
  relay_base: string;
  /** Ist der LAN-Pfad aktiv? Im Doppelmodus sind beide Flags true. */
  lan_enabled: boolean;
  /** Ist der Cloud-Pfad aktiv? */
  cloud_enabled: boolean;
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

/** Daten eines ausgesprochenen „in Vorbereitung"-Aufrufs
 *  (Rust: commands::PreparationCallInfo). */
export interface PreparationCallInfo {
  /** LocationID der Halle, für die gerufen wurde; null = hallenunabhängig. */
  location_id: number | null;
  /** Aufgelöster Hallenname; leer, wenn ohne Halle gerufen wurde. */
  hall: string;
  /** Zeitpunkt des Aufrufs (Unix-Millisekunden). */
  called_at_ms: number;
}

/** Ein „in Vorbereitung" ruf-bares Spiel (Rust: commands::PreparationCandidate). */
export interface PreparationCandidate {
  match_id: number;
  /** Anzeigename, z. B. "HE G1". */
  label: string;
  /** Disziplin als snake_case-Schlüssel (`mens_singles`, `mixed`, …;
   *  leer = unbekannt) — für die Ansage lokalisiert das Frontend selbst. */
  discipline: string;
  /** Name der Auslosung/Klasse (BTP `draw_name`, z. B. „HE A") — für die
   *  Disziplin/Klasse→Halle-Regel (welche Felder erlaubt sind). */
  draw_name: string;
  /** Runden-/Spielbezeichnung (z. B. „G1", „Finale") für die Tabellenanzeige. */
  round_name: string;
  /** Angesetzte Spielzeit (BTP `PlannedTime`) als YYYYMMDDHHMM; null ohne. */
  planned_time: number | null;
  /** Spieler-Namen Team 1 (1 bei Einzel, 2 bei Doppel). */
  team1: string[];
  /** Spieler-Namen Team 2. */
  team2: string[];
  /** Nationalitäten Team 1, parallel zu `team1` (leerer String, wenn
   *  unbekannt) — Grundlage der automatischen DE/EN-Sprachwahl. */
  team1_nationalities: string[];
  /** Nationalitäten Team 2, parallel zu `team2`. */
  team2_nationalities: string[];
  /** Spielnummer (BTP MatchNr), falls vergeben. */
  match_num: number | null;
  /** Aufruf-Daten, falls das Spiel bereits gerufen wurde; sonst null. */
  call: PreparationCallInfo | null;
}

/** Eine Halle des Turniers (Rust: commands::PreparationLocation). */
export interface PreparationLocation {
  id: number;
  name: string;
}

/** Ruf-bare Spiele + Hallen des Turniers (Rust: commands::PreparationView). */
export interface PreparationView {
  candidates: PreparationCandidate[];
  locations: PreparationLocation[];
}

/** Eine Zeile der „Abgeschlossene Spiele"-Tabelle (Rust: commands::FinishedMatchRow). */
export interface FinishedMatchRow {
  match_id: number;
  draw_name: string;
  round_name: string;
  match_num: number | null;
  /** Angesetzte Spielzeit (YYYYMMDDHHMM), null ohne. */
  planned_time: number | null;
  team1: string[];
  team2: string[];
  /** Sieger-Team (1 oder 2). */
  winner: number;
  /** Satz-Ergebnisse als [Team1, Team2]-Paare. */
  sets: [number, number][];
  /** `normal` · `walkover` · `retired` · `disqualified`. */
  result: string;
  /** Feldname, auf dem gespielt wurde (leer = unzugewiesen). */
  court: string;
  /** Halle (leer bei Ein-Hallen-Turnieren). */
  location: string;
  finished_at: number | null;
}
