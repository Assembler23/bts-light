# TP-Datei an badhub — bts-light PR L1 (Dateiwahl, Schreib-Gate, Upload auf Knopfdruck) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Die Turnierleitung wählt im Setup die offene TP-Datei, bindet sie an die Turnier-GUID und sendet sie per Knopf im Dashboard an `POST /api/tp-upload` von badhub; währenddessen hält ein Schreib-Gate die eigenen BTP-Writes kurz zurück, die Datei wird zweimal gelesen und verglichen, der Server-Status landet als Text im Dashboard.

**Architecture:** Alles Dateizugriff, Gate und Netz liegt im Rust-Kern (R1); das Frontend ruft drei Tauri-Commands (`tp_file_check`, `tp_upload_now`, `tp_upload_status`). Neue Module `btp/write_gate.rs` (prozessweites `WriteGate` auf `tokio::sync::RwLock<()>`, ADR 0060) und `badhub/tp_upload.rs` (URL, stabiles Lesen, Senden, Antwort-Enum); `config.rs` bekommt den Block `tp_upload`; `commands.rs` die reine Vorbedingungsprüfung, den Upload-Task und den Status-Merker in `AppState`. badhub-Seite ist fertig und auf Prod (PRs #1045, #1046, #1055, #1057); Vertrag: badhub `docs/features/liveticker_bts.md`, Abschnitt „TP-Datei-Upload `/api/tp-upload`“.

**Tech Stack:** Rust (Tauri 2, tokio mit Feature `sync`, reqwest 0.13 rustls, flate2, `tauri-plugin-dialog`), React 19 + TypeScript (Vite), Handbuch in `docs/*.md` mit `<!-- pruef: -->`-Ankern (`scripts/test-handbuch-fakten.mjs`). Keine neue Abhängigkeit.

**Spec:** `docs/features/tp-datei-upload.md` (Abschnitte „Datei wählen und an die GUID binden“, „Upload auf Knopfdruck“, „Schreib-Gate“, „Rückwärtskompatibilität“, Akzeptanzkriterien 1–11, Tests „bts-light“, Umsetzungs-Hinweis 5), ADR 0060 `docs/adr/0060-upload-sperrt-btp-writes-schreib-gate.md`. Server-Vertrag: badhub `docs/features/liveticker_bts.md`.

## Global Constraints

- **Gesetzt (Grill 26.10.2026):** Quelle ist die **offene** TP-Datei per Dateiwähler, keine BTP-Sicherung. bts-light **schreibt nie** in die Datei (R2). Alles aus der Datei wird sichtbar, der Knopf ist die Freigabe. Verband = Kürzel aus `badhub.live_url` (`aushang::kuerzel_aus_live_url`). L1 ist **nur Knopfdruck**; der Automatik-Takt ist L2.
- **Dateiprüfung** (`tp_file_check` und `lese_stabil`): Endung `.tp` ohne Groß/Klein, Kopf `\x00\x01\x00\x00Standard Jet DB` (19 Bytes), Größe ≤ 32 MB (`TP_MAX_BYTES = 32 * 1024 * 1024`). Größer → „zu groß“, falscher Kopf → „keine TP-Datei“.
- **Gate-Konstanten** (ADR 0060, kommentiert im Modul, nach der Nachlauf-Messung anpassbar): `RUHE = 3 s`, `MAX_WARTEZEIT = 20 s`, `LESEABSTAND = 2 s`, eine Wiederholung bei Abweichung.
- **Request** (Server-Vertrag): `POST <badhub.url ohne letztes Segment>/tp-upload`; Header `Authorization: Bearer <badhub.password>`, `Content-Type: application/octet-stream`, `Content-Encoding: gzip`, `X-Tournament-Uuid: <kanonische GUID>`, `X-Federation: <Kürzel>`, `X-File-Name: <Basisname, percent-kodiert RFC 3986, UTF-8>`; Body `btp::wire::gzip_compress(bytes)`; Timeout **60 s** (eigener Client, nicht `push::build_client`). Status: `GET <tp_upload_url>/status?uuid=<GUID>` mit Bearer, alle 5 s, höchstens 2 Minuten.
- **Antwort-Texte** exakt wie die Tabelle der Spec („Upload auf Knopfdruck“, Punkt 5) und Akzeptanzkriterium 8: 403 bekommt eine **eigene** Variante (nicht wie `push.rs::abgelehnt` mit 401 zusammengelegt); 400/404/Basic-Auth-Wand → „badhub kennt den Upload noch nicht“, kein Banner, kein Warn-Log.
- **Log (R6):** eine Zeile mit Basisname, Rohgröße, komprimierter Größe, Dauer, Statuscode. Nie Inhalt, nie Header-Werte, nie das Passwort.
- **Datenschutz:** Die TP-Datei enthält Anschriften, Telefonnummern, Geburtsdaten. Sie bleibt im Speicher nur für die Dauer des Uploads, es gibt keine Kopie auf Platte, kein Temp-File, kein Logging von Bytes.
- **Slave-Modus:** Abschnitt und Knopf ausgeblendet, `pruefe_upload_bedingungen` lehnt ab („Nur der Master lädt hoch“).
- **Zustand:** `TpUploadStatus` lebt in `AppState` (nicht `SyncEngine`, nicht Config), geht beim Neustart verloren; der Server kennt den Stand.
- **Version** in `Cargo.toml`, `tauri.conf.json`, `package.json` gemeinsam bumpen, Nummer erst kurz vor dem Merge (Memory `bts-light-release-tag-nur-admin`: Tag setzt ein Admin). Handbuch-Anker für Knopf, Abschnittstitel und die drei Meldungen (AK 11). `cargo test` und `npm run build` grün vor jedem Push. Reviews: code-reviewer nach jedem Task; **security-reviewer** (Pflicht laut Spec) für Task 3 (`lese_stabil`, `tp_file_check`, Log-Satz).

## Review Focus

1. **Datei wird während der zwei Lesevorgänge ersetzt** (BTP „Speichern unter“, Datei weg, dann neu): `lese_stabil` darf nicht den ersten Inhalt senden → Task 3 Test „zweite Lesung schlägt fehl → `NichtLesbar`“, nicht `Ok(erste Bytes)`.
2. **Pfad zeigt auf Verzeichnis, Symlink oder 0-Byte-Datei:** `tp_file_check` → „keine TP-Datei“, kein Panic → Task 3 Test (0 Byte, Verzeichnis).
3. **GUID in `tp_upload.tournament_uuid` in anderer Schreibweise als `tournament_uuid`** (`{…}`, klein): Vergleich über die kanonische Form, sonst „Datei neu wählen“ ohne Grund → Task 1 Test.
4. **Zweiter Klick während `laeuft`** und **Klick, während die Sync-Schleife gerade einen Write hält:** No-op bzw. Warten ≤ 20 s, nie zwei Uploads parallel → Task 5 Test (Merker `laeuft` blockt), Task 2 Test (Write hält, Kopie wartet).
5. **Server antwortet 202 und dann nie `result`** (Worker tot): nach 2 Minuten endet `laeuft` mit „angenommen, Verarbeitung dauert noch — Stand beim nächsten Klick“; der Knopf ist wieder frei → Task 5 Test mit Mock, der `result: null` liefert (Zeit per `tokio::time::pause`).

---

### Task 1: `config.rs` — Block `TpUploadConfig` und `tp_datei_fuer_turnier()`

**Files:**
- Modify: `src-tauri/src/config.rs` (neben `PrintConfig` ~Z. 343; `AppConfig` ~Z. 658; Helfer neben `tournament_uuid_kanonisch` ~Z. 1296)
- Test: Unit-Tests im selben Modul (`#[cfg(test)]`, Muster der vorhandenen Roundtrip-Tests)

**Interfaces (Produces):**
```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TpUploadConfig {
    /// Pfad der offenen TP-Datei, wie vom Dialog geliefert; leer = nicht gewählt.
    #[serde(default)] pub path: String,
    /// Turnier-GUID zum Zeitpunkt der Dateiwahl (kanonisch). Bindung, keine zweite Wahrheit.
    #[serde(default)] pub tournament_uuid: String,
}
// AppConfig: #[serde(default)] pub tp_upload: TpUploadConfig,
impl AppConfig {
    /// Pfad nur, wenn die gespeicherte GUID zur aktuellen kanonischen GUID passt; sonst None
    /// ("Datei neu wählen"). Leerer Pfad → None.
    pub fn tp_datei_fuer_turnier(&self) -> Option<PathBuf>;
}
```
`save_config` schreibt den Block unverändert durch (nichts zu tun, serde). Kein Default-Wert außer leer.

- [x] **Step 1: Failing Tests**
```rust
#[test] fn tp_upload_config_roundtrip_und_alte_config_ohne_block() { /* json ohne "tp_upload" lädt mit leerem Block; Roundtrip erhält path+uuid */ }
#[test] fn tp_datei_nur_bei_gleicher_guid() {
    // path gesetzt, tp_upload.tournament_uuid = "{9aa7fe89-…}" (klein, geschweift), tournament_uuid = "9AA7FE89-…" → Some
    // GUID geändert → None; path leer → None; tournament_uuid leer → None
}
```
- [x] **Step 2: `cargo test -p bts-light config::` rot** (Feld fehlt).
- [x] **Step 3: Implementieren** (Vergleich über `is_tournament_uuid`/kanonische Form, dieselbe Normalisierung wie `tournament_uuid_kanonisch`).
- [x] **Step 4: grün. Step 5: Commit** `feat(config): Block tp_upload — TP-Datei an die Turnier-GUID gebunden (L1)`.

---

### Task 2: `btp/write_gate.rs` + Sperre in den vier Write-Funktionen (ADR 0060)

**Files:**
- Create: `src-tauri/src/btp/write_gate.rs`; `mod write_gate;` in `src-tauri/src/btp/mod.rs`
- Modify: `src-tauri/src/tablet/server.rs` `write_result_to_btp` (~Z. 3263), `write_courts_to_btp` (~3312), `write_officials_to_btp` (~3352), `write_highlight_to_btp` (~3388): erste Zeile `let _schreibt = crate::btp::write_gate::GATE.schreiben().await;`
- Test: Unit-Tests im Modul (`#[tokio::test(start_paused = true)]`, eigene Instanzen) + ein Test in `tablet/server.rs` mit blockierendem Mock-BTP (Muster `btp/client.rs`)

**Interfaces (Produces):**
```rust
pub struct WriteGate { lock: tokio::sync::RwLock<()>, last_end_ms: AtomicU64 }
pub static GATE: LazyLock<WriteGate>;
pub const RUHE: Duration = Duration::from_secs(3);
pub const MAX_WARTEZEIT: Duration = Duration::from_secs(20);
#[derive(Debug, PartialEq, Eq)] pub enum GateFehler { Beschaeftigt }
impl WriteGate {
    pub fn new() -> Self;
    /// Lesesperre für die Dauer eines BTP-Writes; Drop stempelt last_end_ms.
    pub async fn schreiben(&self) -> SchreibGuard<'_>;
    /// Schreibsperre für die Kopie: wartet ≤ max_wartezeit auf laufende Writes (sonst Beschaeftigt),
    /// hält dann `ruhe` lang, gibt den Guard zurück; Aufrufer liest darunter und droppt danach.
    pub async fn fuer_kopie(&self, ruhe: Duration, max_wartezeit: Duration) -> Result<KopieGuard<'_>, GateFehler>;
    /// Testsonde: gehaltene Lesesperren.
    pub fn in_flight(&self) -> usize;
    pub fn last_end_ms(&self) -> u64;
}
```

- [x] **Step 1: Failing Tests** (`start_paused`, Zeit per `tokio::time::advance`):
```rust
kopie_wartet_auf_laufenden_write          // Write hält 10 s → fuer_kopie liefert erst nach 10 s + ruhe
write_in_ruhezeit_startet_erst_nach_kopie // schreiben() während gehaltenem KopieGuard blockiert; nach Drop läuft er, nichts verloren
max_wartezeit_ueberschritten_beschaeftigt // Write hält 25 s → Err(Beschaeftigt) nach 20 s
zeitstempel_nach_drop                     // last_end_ms ändert sich beim Drop des SchreibGuard
in_flight_zaehlt                          // 2 gehaltene Guards → 2, nach Drop 0
```
- [x] **Step 2: rot.** **Step 3: Implementieren** (RwLock schreiber-bevorzugt, Begründung aus dem ADR in den Modulkopf; `tokio::time::timeout` um `write()`).
- [x] **Step 4: Sperre in die vier Write-Funktionen**, Test `write_funktionen_halten_die_sperre`: Mock-BTP, der die Antwort 2 s verzögert; während des Writes `GATE.in_flight() == 1` (eigene Instanz nicht möglich → prozessweites `GATE`, Test serialisiert wie der Testsystem-Schalter).
- [x] **Step 5: `cargo test` grün**, Code-Review. **Step 6: Commit** `feat(btp): Schreib-Gate für den TP-Upload — Writes halten Lesesperre, Kopie bekommt garantierte Lücke (ADR 0060)`.

---

### Task 3: `badhub/tp_upload.rs` — URL, `tp_file_check`, `lese_stabil`, `sende`, Antwort-Enum

**Files:**
- Create: `src-tauri/src/badhub/tp_upload.rs`; `pub mod tp_upload;` in `src-tauri/src/badhub/mod.rs`
- Modify: `src-tauri/src/btp/wire.rs` `gzip_compress` → `pub(crate)`
- Test: Unit-Tests im Modul (`tempfile` ist im Dev-Dep-Satz? sonst `std::env::temp_dir()` + eindeutiger Name wie in `tp_live_lib_test`), Mock-Server nach `push.rs::spawn_http_mock_mit_kopfzeile` (~Z. 236)

**Interfaces (Produces):**
```rust
pub const TP_MAX_BYTES: u64 = 32 * 1024 * 1024;
pub const JET_KOPF: &[u8; 19] = b"\x00\x01\x00\x00Standard Jet DB";
pub const LESEABSTAND: Duration = Duration::from_secs(2);
pub const UPLOAD_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Serialize, Clone, PartialEq, Eq)] #[serde(tag = "art", rename_all = "snake_case")]
pub enum TpFileInfo { Erkannt { groesse: u64, anzeige: String /* "Turnierdatei erkannt, 3,6 MB" */ }, KeineTpDatei, ZuGross { groesse: u64 }, NichtGefunden }
pub fn tp_file_check(pfad: &Path) -> TpFileInfo;   // Endung, Existenz, Kopf, Größe — liest nur die ersten 19 Bytes + Metadaten

#[derive(Debug, PartialEq, Eq)] pub enum LeseFehler { Unruhig, KeineTpDatei, ZuGross, NichtLesbar { basisname: String } }
/// Zweimal lesen mit `abstand`, Bytes vergleichen, bei Abweichung einmal wiederholen.
pub async fn lese_stabil(pfad: &Path, abstand: Duration) -> Result<Vec<u8>, LeseFehler>;

pub fn tp_upload_url(badhub_url: &str) -> String;        // wie push::checkin_branding_url: letztes Segment ersetzen → ".../tp-upload"
pub fn status_url(tp_upload_url: &str, guid: &str) -> String; // + "/status?uuid=<GUID>"
pub fn x_file_name(pfad: &Path) -> String;                // Basisname, percent-kodiert (RFC 3986 unreserved bleiben), UTF-8

#[derive(Debug, Clone, Serialize, PartialEq, Eq)] #[serde(tag = "art", rename_all = "snake_case")]
pub enum UploadAntwort {
    Angenommen { job: String }, Unveraendert { received_at: String }, Abgelehnt { result: String, message: String },
    BereitsEndgueltig, ZuSchnell { retry_after_s: u32 }, VerbandFalsch { kuerzel: String }, Unauthorized,
    NochNichtVerfuegbar /* 400, 404, Basic-Auth-Wand */, Serverfehler { message: String } /* 413/415 */, Status(u16), NichtErreichbar,
}
pub fn build_client() -> reqwest::Client;                 // Timeout 60 s
pub struct UploadZiel<'a> { pub tp_upload_url: &'a str, pub password: &'a str, pub guid: &'a str, pub kuerzel: &'a str, pub file_name: &'a str }
pub async fn sende(client: &reqwest::Client, ziel: &UploadZiel<'_>, bytes: &[u8]) -> UploadAntwort; // gzip, Header, Mapping der Status-Tabelle
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)] pub struct StatusAntwort { pub result: Option<String>, pub matches: Option<u32>, pub message: Option<String> }
pub async fn status_abfragen(client: &reqwest::Client, status_url: &str, password: &str) -> Option<StatusAntwort>; // None bei Fehler/404
pub fn antwort_text(a: &UploadAntwort) -> String;         // Dashboard-Texte exakt nach Spec-Tabelle
```
Mapping `sende`: 202 → `Angenommen`; 200 mit `status=unchanged` → `Unveraendert`; 200 `rejected` → `Abgelehnt`; 409 → `BereitsEndgueltig`; 429 → `ZuSchnell` (Header `Retry-After`, Standard 60); 403 → `VerbandFalsch`; 401 ohne Basic-Auth-`WWW-Authenticate` → `Unauthorized`; 401 mit `Basic`, 400, 404 → `NochNichtVerfuegbar`; 413/415 → `Serverfehler{message}`; sonst `Status(n)`; reqwest-Fehler → `NichtErreichbar`.

- [x] **Step 1: Failing Tests**
```rust
tp_file_check_erkannt_klein_und_gross_geschrieben   // "x.tp" und "x.TP" mit Jet-Kopf → Erkannt, anzeige "Turnierdatei erkannt, 0,0 MB"
tp_file_check_falscher_kopf_zu_gross_fehlt_leer_verzeichnis // KeineTpDatei / ZuGross (metadata len > 32 MB per set_len auf Sparse-Datei) / NichtGefunden / 0 Byte → KeineTpDatei / Verzeichnis → KeineTpDatei
lese_stabil_gleich_ok                              // zwei Lesungen gleich → Ok(bytes)
lese_stabil_aenderung_zwischen_lesungen_zweimal_unruhig // Hintergrund-Task schreibt zwischen den Lesungen (Zeit pausiert, advance) → Unruhig
lese_stabil_aenderung_einmal_dann_ruhe_ok           // erste Abweichung, Wiederholung gleich → Ok
lese_stabil_datei_weg_nach_erster_lesung_nicht_lesbar // zweite Lesung schlägt fehl → NichtLesbar{basisname}, NICHT Ok(erste Bytes)
tp_upload_url_prod_und_testsystem                  // "https://badhub.de/api/live_update.php" → ".../api/tp-upload"; "https://test.badhub.de/api/live_update.php" → test
x_file_name_umlaut_und_leerzeichen                 // "Köpi-Cup 2026.TP" → "K%C3%B6pi-Cup%202026.TP"; nur Basisname von "C:\\x\\y\\K.TP"
sende_body_beginnt_mit_gzip_und_header_vollstaendig // Mock zeichnet Request auf: Body 1f 8b, Header X-Tournament-Uuid/X-Federation/X-File-Name/Content-Encoding/Authorization
sende_mapping_aller_statuscodes                    // 202/200 unchanged/200 rejected/409/429+Retry-After/403/401/401 Basic/404/400/413/500 → je Variante
antwort_text_tabelle                               // Texte aus der Spec, z. B. ZuSchnell{30} → "zu schnell hintereinander, in 30 s wieder möglich"; VerbandFalsch{"bvbb"} enthält „1 · Liveticker-Ziel"
```
- [x] **Step 2: rot.** **Step 3: Implementieren.** `lese_stabil` liest mit `tokio::fs::read` (Freigabe: Windows `share_mode` Standard von `std::fs::File::open` erlaubt Lesen bei gehaltener BTP-Datei; falls BTP exklusiv sperrt, zeigt der Feldtest es — dann `NichtLesbar`).
- [x] **Step 4: grün.** **Step 5: Code-Review + Security-Review** (Pfad nur aus dem Dialog, kein Inhalt im Log, Größendeckel vor dem vollständigen Lesen, Passwort nie im Fehlertext). **Step 6: Commit** `feat(badhub): tp_upload — TP-Datei prüfen, stabil lesen, gzip an /api/tp-upload senden, Antworten abbilden (L1)`.

---

### Task 4: `commands.rs` — `pruefe_upload_bedingungen`, `AppState.tp_upload`, Commands `tp_file_check`, `tp_upload_now`, `tp_upload_status`

**Files:**
- Modify: `src-tauri/src/commands.rs` (neben `pruefe_startbedingungen` ~Z. 968; `AppState` ~Z. 64; neue Commands nach `read_tournament_logo`), `src-tauri/src/lib.rs` (`generate_handler!`, drei Einträge)
- Test: Unit-Tests in `commands.rs` für die reine Funktion und den Status-Übergang

**Interfaces (Produces):**
```rust
#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct TpUploadStatus { pub laeuft: bool, pub zeitpunkt_ms: Option<u64>, pub groesse: Option<u64>, pub ergebnis: Option<String> /* fertiger Dashboard-Text */ }
// AppState: pub tp_upload: Mutex<TpUploadStatus>,
/// Reine Funktion, sechs Verletzungen mit eigenem Text und Verweis auf den Setup-Abschnitt.
pub(crate) fn pruefe_upload_bedingungen(config: &AppConfig) -> Result<UploadVorgaben, String>;
pub(crate) struct UploadVorgaben { pub pfad: PathBuf, pub guid: String, pub kuerzel: String, pub tp_upload_url: String, pub password: String }
#[tauri::command] pub fn tp_file_check(path: String) -> Result<TpFileInfo, String>;      // ruft badhub::tp_upload::tp_file_check
#[tauri::command] pub fn tp_upload_now(state: State<'_, AppState>) -> Result<TpUploadStatus, String>; // Vorbedingungen synchron (Err), sonst Task starten, {laeuft:true}
#[tauri::command] pub fn tp_upload_status(state: State<'_, AppState>) -> TpUploadStatus;
```
Reihenfolge der Verletzungen in `pruefe_upload_bedingungen`: (1) `slave_mode` → „Nur der Master lädt die TP-Datei hoch“; (2) `badhub.password` leer → Verweis „3 · Badhub-Zugang“; (3) `tournament_uuid_kanonisch()` None → Verweis „1 · Liveticker-Ziel“; (4) `tp_datei_fuer_turnier()` None, Pfad gesetzt → „Datei neu wählen — die Turnier-GUID hat sich geändert (Setup, Abschnitt „TP-Datei (Turnierdatenbank)“)“; Pfad leer → „Keine TP-Datei gewählt (Setup …)“; (5) `kuerzel_aus_live_url(&badhub.live_url)` None → „Liveticker-Adresse ohne Verbandskürzel — „1 · Liveticker-Ziel“ prüfen“.
Upload-Task (`tokio::spawn`): `GATE.fuer_kopie(RUHE, MAX_WARTEZEIT)` → Err → Text „BTP ist gerade beschäftigt, bitte gleich noch einmal“; `lese_stabil` unter dem Guard, Guard droppen; `tp_file_check`-Regeln auf die Bytes (Kopf, Größe); `sende`; bei `Angenommen` Poll `status_abfragen` alle 5 s bis `result` vorliegt oder 120 s (`ok` → „übernommen, N Spiele“, sonst `message` des Servers; Timeout → „angenommen, Verarbeitung dauert noch — Stand beim nächsten Klick“); `Unveraendert` → „unverändert seit hh:mm“ aus `received_at` (UTC → lokale Zeit); am Ende `laeuft = false`, `zeitpunkt_ms`, `groesse`, Log-Zeile (`tracing::info!` mit Basisname, Rohgröße, gzip-Größe, Dauer, Status). Zweiter Klick bei `laeuft` → aktueller Status zurück, kein zweiter Task.

- [x] **Step 1: Failing Tests**
```rust
pruefe_upload_bedingungen_alle_sechs_verletzungen // je Verletzung Err mit dem Verweis-Text; vollständige Config → Ok mit pfad/guid/kuerzel/url
tp_upload_status_default_nicht_laufend
upload_task_setzt_laeuft_zurueck_und_text         // Mock 202 + Status-Mock mit result ok/matches 7 → ergebnis "übernommen, 7 Spiele", laeuft false
upload_task_202_ohne_result_endet_nach_2_minuten  // Status-Mock result null, Zeit pausiert → Text „angenommen, Verarbeitung dauert noch …", laeuft false
zweiter_klick_waehrend_laeuft_ist_noop            // Merker laeuft=true → tp_upload_now liefert Status, startet nichts (Zähler am Mock bleibt 1)
```
(Der Task wird für Tests als freie Funktion `fuehre_upload_aus(vorgaben, status: Arc<Mutex<TpUploadStatus>>, gate: &WriteGate, client)` gebaut; der Command ruft sie mit `GATE` und `AppState`.)
- [x] **Step 2: rot. Step 3: Implementieren.** `lib.rs`: Commands registrieren. **Step 4: grün**, Code-Review. **Step 5: Commit** `feat(commands): tp_upload_now/tp_upload_status/tp_file_check — Vorbedingungen, Upload-Task mit Schreib-Gate, Status-Merker (L1)`.

---

### Task 5: Frontend — `types.ts`, `api.ts`, Setup-Abschnitt, Dashboard-Abschnitt

**Files:**
- Modify: `src/types.ts` (`TpUploadConfig` an `AppConfig.tp_upload`, `TpUploadStatus`, `TpFileInfo`), `src/api.ts` (`tpFileCheck(path)`, `tpUploadNow()`, `tpUploadStatus()`)
- Modify: `src/pages/SetupWizard.tsx`: neuer Abschnitt **„TP-Datei (Turnierdatenbank)“** direkt hinter „2 · BTP-Verbindung“ (~Z. 1221), Muster `pickLogo` (~Z. 678) mit `open({ filters: [{ name: "Turnierdatei", extensions: ["tp", "TP"] }] })`; Zustand `tpPath`, `tpInfo`; beim Wählen `tpFileCheck` → nur bei `erkannt` wird `tp_upload = { path, tournament_uuid: <aktuell eingetragene GUID, kanonisch> }` in die zu speichernde Config übernommen; Knopf „Entfernen“ leert beide; Hinweis „Datei neu wählen“, wenn gespeicherte GUID ≠ aktuelle GUID; Hinweis „Die Datei liegt auf dem BTP-Rechner — Upload nur von dort empfohlen“, wenn `btp.host` ∉ {`127.0.0.1`, `localhost`}; im Slave-Modus nicht gerendert
- Modify: `src/pages/Dashboard.tsx`: Abschnitt **„Turnierdaten an badhub“** vor „Aushang für die Halle“ (~Z. 488): Knopf **„TP-Datei an badhub senden“**, Statuszeile (`ergebnis`, bei `laeuft` „läuft …“), Hinweistext „Alles aus der Datei wird auf badhub sichtbar — auch noch nicht veröffentlichte Auslosungen.“; ohne Pfad oder GUID-Abweichung Knopf `disabled` + Link ins Setup („Datei neu wählen“ bzw. „Im Setup eine TP-Datei wählen“); Poll `tpUploadStatus` alle 5 s nur solange `laeuft` (Muster `PrintWarningBanner.tsx` `useEffect` + `setInterval`); im Slave-Modus nicht gerendert
- Test: `npm run build` (tsc) grün; manuelle Prüfung AK 1–5

- [x] **Step 1: Typen und API-Wrapper** (Feldnamen exakt wie die Rust-Serialisierung: `art`-Tag, `laeuft`, `zeitpunkt_ms`, `groesse`, `ergebnis`).
- [x] **Step 2: Setup-Abschnitt**, **Step 3: Dashboard-Abschnitt**, **Step 4: `npm run build`** grün, `cargo tauri dev` Sichtprüfung (Knopf aus ohne Pfad; „Datei neu wählen“ nach GUID-Änderung; Slave-Modus blendet aus).
- [x] **Step 5: Code-Review. Step 6: Commit** `feat(ui): TP-Datei im Setup wählen, Knopf „TP-Datei an badhub senden“ im Dashboard (L1)`.

---

### Task 6: Handbuch, Anker, Doku, Changelog, Roadmap, Version

**Files:**
- Create: `docs/tp-datei-upload.md` (Handbuch: Zweck, Voraussetzungen, Datei wählen, Knopf, Meldungstabelle, Hinweis Sichtbarkeit/Auslosungen, UNC-Pfad-Hinweis, „nur der Master“) mit Ankern: `<!-- pruef: "TP-Datei an badhub senden" in src/pages/Dashboard.tsx -->`, `<!-- pruef: "TP-Datei (Turnierdatenbank)" in src/pages/SetupWizard.tsx -->`, `<!-- pruef: "BTP ist gerade beschäftigt" in src-tauri/src/commands.rs -->`, `<!-- pruef: "badhub kennt den Upload noch nicht" in src-tauri/src/badhub/tp_upload.rs -->`, `<!-- pruef: "Turnier bereits endgültig importiert" in src-tauri/src/badhub/tp_upload.rs -->`; Eintrag in `docs/handbuch.json` (Whitelist)
- Modify: `docs/einstellungen.md` (Zeile `tp_upload.path`/`tournament_uuid`, Standard leer), `docs/erste-schritte.md` (Schritt „TP-Datei wählen“ optional), `docs/oberflaechen.md` (Dashboard-Abschnitt, Setup-Abschnitt), `docs/multi-hall.md` („Nur der Master lädt hoch“), `docs/btp_protocol.md` (Absatz „Schreib-Gate“: neue Wartestelle für BTP-Writes, Konstanten), `docs/features/testsystem-umschalter.md` (Upload-URL folgt `badhub.url`), `docs/features/tp-datei-upload.md` (Status-Zeile: „bts-light L1 umgesetzt v0.9.x, L2 offen“; Offene Fragen „Messung A2“ und „Jet-Nachlauf“ als Pflicht vor dem Feldtest stehen lassen), `docs/adr/0060-…md` (Nachtrag „Umgesetzt in PR L1, Konstanten unverändert bis zur Messung“), `docs/roadmap.md` (Eintrag „Spezifiziert“ → „Umgesetzt (L1), L2 offen“), `docs/changelog.md` (neuer Block `## v0.9.300` oder höher: „**TP-Datei an badhub.** Im Setup lässt sich die offene Turnierdatei wählen, im Dashboard sendet ein Knopf sie an badhub; dort entsteht noch am Turniertag ein vorläufiger Stand mit Auslosungen, Gruppen, KO-Bäumen und Spielliste. Alles aus der Datei wird sichtbar — Auslosungen also erst senden, wenn sie veröffentlicht werden dürfen. Während des Sendens wartet bts-light kurz mit eigenen BTP-Schreibzugriffen.“), `CLAUDE.md` (Doku-Tabellenzeile aus der Spec, Abschnitt „Betroffene Doku-Dateien“, wörtlich übernehmen)
- Version: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `package.json` gemeinsam (Nummer erst kurz vor dem Merge gegen `origin/main` prüfen)

- [x] **Step 1: Handbuch + Anker schreiben; `node scripts/test-handbuch-fakten.mjs` grün; `node scripts/test-handbuch.mjs` (Whitelist) grün.**
- [x] **Step 2: übrige Doku, Changelog, Roadmap, CLAUDE.md.**
- [x] **Step 3: `cargo test`, `npm run build`, Version bumpen. Commit** `docs(tp-upload): Handbuch „TP-Datei an badhub“, Anker, Changelog, Roadmap, Version (L1)`.
- [x] **Step 4: Push, PR** (Body: Was, Entscheidungen, Tests, Feldtest-Voraussetzungen: Messung A2 und Jet-Nachlauf laut Spec vor dem ersten Einsatz bei einem echten Turnier; der Testzugang `lehiero` ist in badhub auf `bvbb` gemappt, Probelauf gegen Prod mit der Messdatei möglich).

## Self-Review (durchgeführt)

- **Spec-Abdeckung L1:** Datei wählen + Bindung (Task 1, 5), Schreib-Gate (Task 2), Lesen/Senden/Antworten (Task 3), Vorbedingungen/Task/Status (Task 4), Setup- und Dashboard-Abschnitte (Task 5), Handbuch/Anker/Doku/Changelog/Roadmap/Version/ADR-Nachtrag (Task 6). AK 1–11 je einem Task zugeordnet; AK 6 (hängender Write 10 s / 20 s) in Task 2; AK 7 in Task 3; AK 8–10 in Task 3/4; AK 11 in Task 6. L2 (Automatik) bewusst nicht enthalten.
- **Entscheidungen über den Spec-Wortlaut hinaus:** (a) `UploadAntwort` bekommt zusätzlich `Serverfehler{message}` für 413/415 und `NichtErreichbar` (Spec nennt die Texte, aber keine Varianten); (b) Upload-Logik als freie Funktion `fuehre_upload_aus` mit injiziertem Gate/Client, damit Task 4 ohne Tauri-Runtime testbar ist; (c) der Dialogfilter nennt `tp` und `TP` (Linux-Dialoge filtern groß/klein getrennt); (d) `tp_file_check` liest nur 19 Bytes und Metadaten, die volle Lesung passiert erst im Upload unter dem Gate; (e) Zeitstempel „unverändert seit hh:mm“ in lokaler Zeit des Rechners.
- **Typen konsistent:** `TpUploadConfig{path,tournament_uuid}` (Task 1 ↔ 5), `TpFileInfo`/`UploadAntwort`/`StatusAntwort` (Task 3 ↔ 4 ↔ 5), `TpUploadStatus{laeuft,zeitpunkt_ms,groesse,ergebnis}` (Task 4 ↔ 5), `GATE`/`RUHE`/`MAX_WARTEZEIT` (Task 2 ↔ 4), `LESEABSTAND` (Task 3), Texte der Anker (Task 4/3 ↔ 6).
- **Proportion:** Plan ≈ Spec-Abschnitt bts-light; Code nur als Signaturen und Testnamen.
