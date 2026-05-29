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
  | { kind: "info_overview" }
  | { kind: "info_preparation" }
  | { kind: "ad_rotation" }
  | { kind: "ad_single"; file: string };

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
  /** Einstellungen der gesprochenen Feld-Ansagen. */
  announce: AnnounceConfig;
  /** Einstellungen der Court-Monitor-Anzeige (TV am Spielfeld). */
  court_monitor: CourtMonitorConfig;
}

export interface SyncStatus {
  running: boolean;
  /** "idle" | "ok" | "btp_error" | "push_error" */
  kind: string;
  message: string;
  updated_at_ms: number;
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
