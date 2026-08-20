# CLAUDE.md – bts-light Projektwissen

**bts-light** = Plug-and-play-Brücke zwischen **BTP** (Badminton Tournament
Planner) und dem **badhub.de-Liveticker**, plus digitaler Tablet-Spielzettel
für Schiedsrichter. Windows-Desktop-App, gedacht als Ablösung von
`letilo/bts`. Zielgruppe: Turnierleiter ohne technischen Hintergrund —
installieren, BTP verbinden, Badhub-Passwort eintragen, fertig.

Repo: `Assembler23/bts-light` (public). Arbeitsbranch: `main`.

## Stack & Aufbau

- **Tauri 2** — Windows-App mit nativem WebView.
- **Rust** (`src-tauri/`) — App-Kern: BTP-Protokoll, Liveticker-Push,
  Tablet-Server/Relay-Client, Tauri-Commands.
- **React 19 + Vite + TypeScript + Tailwind 4** (`src/`) — Setup-/Dashboard-UI.
- **Cargo-Workspace** mit drei Crates:
  - `src-tauri/` — die App (Binary `bts-light`).
  - `relay/` — eigenständiger WebSocket-Broker (`bts-relay`), läuft auf
    dem Hetzner-Server hinter nginx `/bts-relay/`.
  - `relay-proto/` — geteilte JSON-Wire-Typen zwischen App und Relay.

## Architektur – feste Regeln

**R1** Das WebView-Frontend spricht den Rust-Kern **ausschließlich** über
Tauri-Commands an (`src/api.ts` ↔ `src-tauri/src/commands.rs`). Kein
direkter BTP-/Netzwerkzugriff aus React.

**R2** **BTP ist die Wahrheit.** Matches, Courts und Zuordnungen kommen per
`SENDTOURNAMENTINFO`; Ergebnisse gehen per `SENDUPDATE` zurück. Frontend
und Tablets erfinden keine Court→Match-Zuordnung.

**R3** Zwei Tablet-Verbindungsarten, umschaltbar im Setup: **LAN**
(eingebetteter Server `0.0.0.0:8088`) oder **Cloud** (Relay auf badhub.de,
nur ausgehende Verbindungen — funktioniert hinter Firmen-Firewalls). Der
Modus-Wechsel greift beim nächsten Stoppen/Starten.

**R4** Cloud-Relay: genau **ein Host** pro Namespace, **ein aktives Tablet**
pro Court. Namespace = `install_id`.

**R5** `process_result` (server.rs) validiert **jedes** eingehende Ergebnis
(Match-ID muss zum Court-Match passen, Satzplausibilität) — das ist
zugleich die Sicherheits-Mitigation des Cloud-Modus. LAN- und Cloud-Pfad
teilen sich diese Logik.

**R6** `install_id` ist eine zufällige UUID, einmalig vom Frontend erzeugt.
Sie ist **gleichzeitig** der Relay-Namespace **und** die Zuordnung der
hochgeladenen Diagnose-Logs.

## Coding-Standards

- Rust: idiomatisch, `cargo test` grün vor jedem Commit. Kommentare
  **Deutsch** (was + warum, nicht wie).
- React/TS: `npm run build` (= `tsc && vite build`) muss fehlerfrei sein.
- **TDD**: jedes Feature bekommt Rust-Unit-Tests (z. B. `relay-proto`-Serde-
  Roundtrips, Broker-Routing, Parser-Regressionen).
- **Version bumpen**: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`
  und `package.json` immer gemeinsam auf dieselbe Version.

## Release & Auto-Update

- Tag `vX.Y.Z` pushen → GitHub-Actions `release.yml` baut den Windows-
  Installer + signierte Tauri-Updater-Artefakte und publiziert `latest.json`
  nach `badhub.de/download/bts-light/`.
- Auto-Update-Endpoint: `https://badhub.de/download/bts-light/latest.json`.
- **Stabil halten:** Tauri-`identifier` `de.badhub.btslight` und der
  Updater-Pfad `download/bts-light/` — Änderungen brechen das Auto-Update
  bestehender Installationen.
- `public/download/bts-light/` auf dem badhub-Server ist **nicht** im
  badhub-Repo; der badhub-Deploy nimmt `public/download/` vom rsync aus.
- Details: [docs/release.md](docs/release.md).

## Dokumentations-Pflicht beim Commit

Feature/Bugfix → zuständige `docs/**/*.md` im selben Commit pflegen.

| Code-Pfad | Doku-Datei |
|---|---|
| `src-tauri/src/btp/*` | `docs/btp_protocol.md` |
| `src-tauri/src/tablet/server.rs`, `assets/tablet.html` | `docs/tablet.md` |
| `src-tauri/src/tablet/relay_client.rs`, `relay/`, `relay-proto/` | `docs/cloud-relay.md` |
| Walkover (`tablet/state.rs`, `server.rs`, `commands.rs` `walkover_*`) | `docs/walkover.md` |
| **Zähltafelbediener** (`tablet/state.rs` `ScorekeeperEntry`/`*_scorekeeper`, `sync.rs` `track_scorekeepers`, `commands.rs` `scorekeeper_*`, `config.rs` `ScorekeeperConfig`, `pages/FieldOverviewPage.tsx`) | `docs/zaehltafelbediener.md` |
| **Spiele in Vorbereitung** (`tablet/state.rs` `PreparationCall`, `commands.rs` `preparation_*`, `badhub/payload.rs` `preparation_call_ts`/`hall`, `badhub/diff.rs` Fingerabdruck, `pages/PreparationPanel.tsx`) | `docs/preparation.md` |
| **Hallen-Check-In** (`btp/model.rs` `BtpEvent`/`BtpEntry`/`entry_list`/`non_main_stage_entries` (Hauptfeld-Filter über `StageEntries`), `badhub/payload.rs` `centry_list`, `badhub/diff.rs` `roster_update`, `sync.rs` `push_checkin_roster` + `fetch_checkin_times` (TL-Panel-Lese-Takt), `badhub/checkin_state.rs`, `commands.rs` `checkin_*`, `config.rs` `CheckinConfig`, `tablet/state.rs` `checkin_classes`, `tablet/tl.rs` `TlCheckinTime`/`checkin_times_heute` + Panel „Anfangszeiten" in `assets/tl.html`, `src/tournamentGuid.ts`, `src/io/checkinPairs.mjs`, `pages/CheckinPanel.tsx`, Check-In-Abschnitt im `SetupWizard`) | `docs/spieler-check-in.md` · Bedienung TL-Panel: `docs/turnierleitung-web.md` |
| **Turnierleitungs-Oberfläche (TL-Web)** (`relay-proto` `Tl*`-Typen/Frames, `config.rs` `TlWebConfig`/`TlDevice`, `commands.rs` `identity_bundle`/`keep_host_managed_fields`, `tablet/tl.rs`, `tablet/assign.rs`, `assets/tl.html`, `pages/TlWebPanel.tsx` (Geräteverwaltung), TL-Routen in `server.rs` + `relay/`) | `docs/turnierleitung-web.md` (Bedienung) · `docs/features/turnierleitung-web.md` (Spec) · `docs/cloud-relay.md` (Wire-Ebene, Token) |
| **TL-Web Panels & Profile** (`config.rs` `TlPanelProfile`/`TlPanelSetting`/`TlDisplaySettings`/`TlDevice.profile_id`, `relay-proto` `TlAction::Profile*`/`TlPanelProfileWire`/`TlAuthDevice.profile_id`/`MAX_TL_PROFILES`, `tablet/tl.rs` `profiles_view`/`profile_save`/`profile_delete`/`profile_select`/`profile_set_default`, `X-Tl-Active-Profile`-Header in `tablet/server.rs` + `relay/`, `commands.rs` `mutate_config_at`, Panel-/Profil-Teil von `assets/tl.html`) | `docs/features/tl-web-panelsystem.md` (Spec) · `docs/turnierleitung-web.md` (Bedienung) · `docs/cloud-relay.md` (Wire-Ebene) |
| **Feld-Raster** (`config.rs` `hall_layouts`/`HallLayoutConfig`, `src/io/hallGrid.mjs` `gridPositions`, `pages/FieldOverviewPage.tsx`, `tablet/tl.rs` `TlHallLayout`, Raster+Listen-Position in `assets/tl.html`) | `docs/features/feld-raster.md` (Spec) · `docs/turnierleitung-web.md` (Bedienung) |
| **Manuelle Spielreihenfolge** (`tablet/queue_order.rs` `QueueOrderStore`, `tablet/assign.rs` `sort_key_with_manual_order`/`resolve_and_sort_key`/`ready_queue`, `sync.rs` `reconcile_queue_order`, `relay-proto` `TlAction::QueueReorder`/`QueueOrderReset`, `tablet/tl.rs`/`commands.rs`/`tablet/server.rs`/`badhub/payload.rs` Sortier-Umstellung, `assets/tl.html` `enableReorderDrag` (eine globale Liste, ADR 0026), `pages/PreparationPanel.tsx`, `tests/queue_order_consistency.rs`) | `docs/features/spielliste-manuelle-reihenfolge.md` (Spec) · `docs/btp_protocol.md` (Sortier-Definition) |
| **Spielzeiten & Prognose** (`tablet/match_times.rs` (Store `match-times.json`, E4-Reset), `tablet/predict.rs` (Median-Statistik + Simulation; Etappe D: `live_remaining_min`/`group_times` — Restzeit aus Live-Satzstand, `TlCourt.remaining_min`, Schalter `TlDisplaySettings.show_court_remaining`), `sync.rs` `reconcile_match_times` + Prognose-Kontrolle, E2-Stempel in `server.rs` `handle_score`, E3-Stempel + Duration-Quelle in `process_result`/`commands.rs` `enter_result`/`disqualify_match`/`tl.rs` `execute_result_action` (Walkover bewusst 0), `config.rs` `PredictionConfig`, TlState-Felder `predicted_*`/`time_stats`/`brutto_mins` in `tl.rs`, Panel „Spielzeiten" + Prognose-Marke in `assets/tl.html`, SetupWizard-Abschnitt; **Pausen-Teil:** Pause hält bis „Weiterspielen" + `startedAt` in `assets/tablet.html`, `TlPause` (optionales `ends_at_ms`/`started_at_ms`) in `tl.rs`, Pausen-Countdown/„überzogen" in `assets/tl.html`) | `docs/spielzeiten-prognose.md` (Bedienung) · `docs/features/spielzeiten-prognose.md` (Spec) · `docs/btp_protocol.md` (Duration) · `docs/turnierleitung-web.md` · `docs/tablet.md` (Pausen) · `docs/cloud-relay.md` · ADR 0027/0028 |
| **Hallen-Vorverteilung** (`tablet/hall_assign.rs` (`AutoHallStore` `auto-halls.json` + `distribute`/`effective_window`), `assign.rs` `HallSource::Auto` + `auto_hall`-Parameter der Kaskade, `sync.rs` `reconcile_auto_halls` + Hallen-Bindung in `auto_assign` (ADR 0030), E3-Räumen in `tl.rs` `CallPreparation`/`SetHall` + `commands.rs` `call_preparation`, Auto-Stempel in `state.rs` `apply_preparation_calls`, relay-proto `TlAction::SetHallPrefill`/`ClearAutoHalls`, `config.rs` `HallPrefillConfig`, `TlState.hall_prefill` + Bedien-Cluster/Badge in `assets/tl.html`) | `docs/features/hallen-vorverteilung.md` (Spec) · `docs/turnierleitung-web.md` (Bedienung) · `docs/multi-hall.md` · `docs/btp_protocol.md` (Kaskade/Vergabe) · `docs/cloud-relay.md` · ADR 0029/0030 |
| **Schiri-Modus am Tablet** (`assets/tablet.html` `STATE.umpireMode`/`ump*`-Ansagetexte/`openCardModal`/`applyCard`/`roteKarteMoeglich`, PIN-Gate im Einstellungs-Menü) | `docs/umpire-mode.md` |
| **Schiedsrichterzettel** (`relay-proto` `MatchEvent`/`EventKind`/`Phase`/`MatchEvent`-Frames/`ScoresheetRequest`/`ScoresheetData` + Deckel `MAX_EVENTS_PER_MATCH`/`MAX_SHEET_LEN`/`MAX_EVENT_ID_LEN`/`MAX_SHEETS_PER_DOC`, `tablet/sheet.rs` `SheetStore` (append-only, **vereinigt** statt zu ersetzen), `sheet_store()` in `tablet/state.rs`, Verzeichnis `zettel/` in `commands.rs`, `tablet/scoresheet.rs` `sheet_grid`/`render_html`/`html_fuer`, `tl_scoresheet`-Routen in `server.rs` + `relay/`, `commands.rs` `match_scoresheet_html`, `src/io/matchEvents.mjs` + `scripts/test-match-events.mjs` + Inline-Kopie in `assets/tablet.html`, `erfasseEreignis`/`roteKarteMoeglich`/`sendMatchEvent*` in `assets/tablet.html`) | `docs/features/schiedsrichterzettel-druck.md` (Spec) · `docs/cloud-relay.md` (Wire-Ebene) · ADR 0037/0038/0039 |
| **Punktverlauf-Graph** (`relay-proto` `Rally`/`RallySync`/`TimelineRequest`/`MatchTimeline`, `tablet/timeline.rs` `TimelineStore`, Ingest in `tablet/server.rs`/`relay_client.rs`, `tl_timeline`-Routen in `server.rs` + `relay/`, `commands.rs` `match_timeline`, `timelineSetSvg` in `assets/tablet.html`+`tl.html`, `components/TimelineChart.tsx`) | `docs/punktverlauf.md` · Spec `docs/features/punktverlauf-graph.md` |
| **Perf-Messung der Anzeige-Strecke** (`tablet/perf.rs` `PerfCounters`/`PerfSnapshot`/`PerfLog`, `tablet/state.rs` `perf()` + Vermerke in `overview`/`persist_scores`/`notify_monitor`, `tablet/server.rs` `src`-Query in `DeviceHeartbeat`/`DeviceQuery` + `/debug/perf` (**nur LAN**), `sync.rs` 10-s-Zeile in `run_once`, `scripts/last-monitor.mjs`, `ovRenderMessen` in `assets/overview.html`) | `docs/features/monitor-livestand-push.md` (Spec, Etappe S0) · `docs/court-monitor.md` (Bedienung) · `docs/logging.md` (Log-Zeile) · `docs/regression-suite.md` (manueller Lauf) |
| **Bestätigung „nichts Neues" am Relay** (`relay/` `uebersicht_marke` + `marke_passt`-Zweig in `overview_health`) | `docs/features/monitor-livestand-push.md` (Spec, Etappe S8) · `docs/court-monitor.md` |
| **Antwortcache am Relay** (`relay/` `OverviewCache`/`OVERVIEW_CACHE_TTL_MS`, `Namespace.overview_rev`/`overview_cache`/`overview_builds` + `bump_overview_rev` in `notify_monitor`, `monitor_upload` und beim `HostFrame::Courts`-Arm) | `docs/features/monitor-livestand-push.md` (Spec, Etappe S9) · `docs/court-monitor.md` |
| **Schmaler Abruf je Feld** (`tablet/server.rs` `DeviceHeartbeat.court` + `alle_feld_ausschnitte`, `state.rs` `FeldCache`/`feld_ausschnitt` (faul, je Cache-Generation), `relay/` `OverviewQuery` + Filter in `overview_health`, `--schmal` in `scripts/last-monitor.mjs`) | `docs/features/monitor-livestand-push.md` (Spec, Etappe S7) · `docs/court-monitor.md` |
| **Gesundheit des Push-Kanals** (`relay-proto` `MONITOR_HEARTBEAT_MS`/`MONITOR_HEARTBEAT_STALE_MS`/`monitor_heartbeat_frame`/`MonitorConfig.push_fallback_slow`, Herzschlag in `tablet/server.rs` `monitor_socket` + `relay/` `monitor_conn`, `config.rs` `CourtMonitorConfig.push_fallback_slow` + `tablet/monitor.rs` `to_monitor_config`, `pushFallbackSlow` im `callTimer` von `/health` (LAN **und** `relay/` `overview_health`), `relay/` `namespace_aufraeumen` + `subscribe_monitor`-Absage, `src/io/pushHealth.mjs` + `scripts/test-push-health.mjs` + Inline-Kopien (`mwsTrennen`/`mwsOffenSeitMs`) in `assets/overview.html`/`monitor.html`) | `docs/features/monitor-livestand-push.md` (Spec, Etappe S6) · `docs/court-monitor.md` (Herzschlag, Schalter) |
| **Teil-Patch der Feld-Übersicht** (`src/io/courtPatch.mjs` `istPatchbar`/`brettSignatur` + `scripts/test-court-patch.mjs`, Inline-Kopie und `patcheKarten`/`satzSpalte`/`sichtSignatur`/`renderMitPatch` in `assets/overview.html`) | `docs/features/monitor-livestand-push.md` (Spec, Etappe S5) · `docs/court-monitor.md` |
| **Anzeige-Ordnung `seq`** (`relay-proto` `MonitorState.seq`, `tablet/state.rs` `monitor_seq_of`/`CourtOverview.seq`/`MonitorCourt.seq` + Seeding über `now_ms()` in `notify_monitor`, `tablet/monitor.rs`, `relay/` `seq` in `overview_health`/`build_monitor_state` + gleiches Seeding, `src/io/monitorSeq.mjs` + `scripts/test-monitor-seq.mjs` + Inline-Kopien in `assets/overview.html`/`monitor.html`) | `docs/features/monitor-livestand-push.md` (Spec, Etappe S4) · `docs/court-monitor.md` (Ordnung) |
| **Zuweisungs-Nudge** (`tablet/state.rs` `monitor_belegung`/`nudge_belegungs_wechsel` + Projektion in `set_snapshot`, `relay/` Nudge in den Armen `MatchAssigned`/`MatchCleared`) | `docs/features/monitor-livestand-push.md` (Spec, Etappe S3) · `docs/court-monitor.md` (WS-Nudge, beide Betriebsarten) |
| **Entprellter Live-Stand** (`tablet/state.rs` `scores_dirty`/`scores_fingerprint`/`mark_scores_dirty`/`flush_scores` + synchroner Flush in `clear_court`/Feldwechsel, Sekundentakt im Sync-Loop `commands.rs`, Flush in `stop_sync` und `lib.rs` `beenden`) | `docs/features/monitor-livestand-push.md` (Spec, Etappe S2) · `docs/tablet.md` (Wahrheit beim Tablet) |
| **Antwortcache der Übersicht** (`tablet/state.rs` `OverviewCache`/`overview_rev`/`bump_overview_rev` + Meldung in `notify_monitor`/`set_snapshot`, `tablet/server.rs` `uebersicht_json`/`OVERVIEW_CACHE_TTL_MS` + ETag/304 in `health`, `commands.rs` Meldung in `mutate_config`/`save_config`, Uhr-Versatz + Uhr-Takt in `assets/overview.html`) | `docs/features/monitor-livestand-push.md` (Spec, Etappe S1) · `docs/court-monitor.md` (Bedienung) |
| **TL-Web-Push** (`tablet/state.rs` `tl_subs`/`subscribe_tl`/`notify_tl`/`TlStateCache`, `tablet/server.rs` `tl_push_takt`/`tl_ws_socket` + Cache-Zweig in `tl_state`, `relay/` `tl_subs`/`notify_tl`/`tl_ws_conn`, Push-Teil von `assets/tl.html` (`verbindePush`/`setzePollTakt`)) | `docs/features/tl-web-push.md` (Spec) · `docs/turnierleitung-web.md` (Bedienung) · `docs/cloud-relay.md` (Wire-Ebene) · ADR 0034 |
| Sprachansagen (`io/announcer.ts`, `components/MatchAnnouncer.tsx`, `Discipline`, Ansage-Knopf in `PreparationPanel`) | `docs/announcements.md` |
| Court-Monitor (`tablet/monitor.rs`, `tablet/mdns.rs`, `assets/monitor.html`, `assets/overview.html`, `assets/preparation.html`, `assets/flags/`, Court-/Monitor-/`/info/*`-Routen in `server.rs` + `relay/`, `pages/CourtMonitorPanel.tsx`, `monitor_*`-Commands) | `docs/court-monitor.md` |
| **Sponsor-Leiste** (Werbebild klein neben dem Turnierlogo in den Kopfleisten: `monitor.rs` `AD_BAR_FILE`/`read_write_ad_bar`, `commands.rs` `set_court_ad_bar`, `/info/logo` + `barAds` in `server.rs`, `sponsor-bar` in `assets/{overview,preparation,monitor,tablet}.html`, `SetupWizard`-Ad-Häkchen; **badhub-Check-In-Push:** `commands.rs` `collect_bar_sponsors_b64`/`push_bar_sponsors_to_badhub`/`push_logo_to_badhub`/`spawn_branding_push` (+ Logo-Auslöser in `save_config`), `badhub/push.rs` `checkin_branding_url`/`push_checkin_branding`, `badhub/payload.rs` `CheckinBrandingMessage`) | `docs/court-monitor.md` · Spec `docs/features/werbung-leisten.md` · Push-Schema `docs/spieler-check-in.md` |
| **Werbe-Hintergrund + Feldbezeichnung** (Anzeige-Stil je Werbebild im Leerlauf-Vollbild: `monitor.rs` `AD_STYLE_FILE`/`AdStyle`/`read_write_ad_style`/`schriftfarbe`/`ad_styles_fuer`, `hall_colors.rs` `ist_hex_farbe` (geteilter Farb-Wächter), `commands.rs` `set_court_ad_style` + `CourtAd.bg`/`show_court`, `server.rs` `ad_style()`/`style_cache` + `adStyles` in `/info/ad/state`, `relay_client.rs` `ad_abdruck_zeile` (Fingerabdruck!) + `AdUpload.style`, `relay-proto` `AdStyleWire`/`MonitorState.ad_styles`, `relay/` `AdImage.style`, `#ad-court`/`mit-feld`/`adFarbe` in `assets/monitor.html`, `setzeStil` in `assets/ad.html`, Farbwähler + Vorschau in `pages/SetupWizard.tsx`) | `docs/court-monitor.md` · Spec `docs/features/werbung-hintergrund-und-feld.md` · `docs/cloud-relay.md` (Wire-Ebene) · ADR 0041 |
| **Mehr-Hallen-Architektur** (CourtID-Identität in `btp/model.rs` + `tablet/state.rs`, Hallen-Gruppierung in den UIs, `ConnectionMode::LanAndCloud` in `config.rs`, `merge_device_lists` in `relay-proto`, Slave-Monitor-Brücke `tablet/slave_bridge.rs`) | `docs/multi-hall.md` |
| **Hallen-Farben** (`hall_colors.rs` Palette/Resolver/`paint`/`view`, `config.rs` `HallColorConfig`/`hall_colors`/`upsert_hall_color`, `commands.rs` `set_hall_color`/`remove_hall_color`/`hall_colors_view` + `keep_host_managed_fields`, `tablet/state.rs` `CourtOverview.hall_color`, Farb-Picker in `pages/FieldOverviewPage.tsx`, Marke in `pages/PreparationPanel.tsx`, `tl.rs` `TlHall.color`/`TlFinished.hall`, Marken in `assets/tl.html` + Monitor-Seiten, relay-proto `hall_color`-Felder, `badhub/payload.rs` `hall_color`) | `docs/features/hallen-farben.md` (Spec) · `docs/multi-hall.md` (Konzept+Bedienung) · `docs/turnierleitung-web.md` · `docs/court-monitor.md` · `docs/cloud-relay.md` · ADR 0031/0032/0033 |
| **Schiedsrichtermanagement** (`btp/model.rs` `BtpOfficial`/`official_list`/`official1_id`/`official2_id`, `config.rs` `OfficialsConfig`, `tablet/officials.rs` `OfficialsStore` (turniergebundene `officials-state.json`, `official_conflict`, `rotate_court`, `confirm`), `state.rs` `officials_store()`/`court_officials`/`officials_for_write`, `sync.rs` `track_officials`/`reconcile_officials`/`officials_entries`, `btp/proto.rs` `officials_request` + `MatchCourt::officials`, `commands.rs` `official_*`, `tablet/tl.rs` `TlOfficial`/`official_detail_json` + Action-Arme, `relay-proto` `TlOfficialRole`/`OfficialDetail*`/`MatchBrief::sr_names`, `/tl/api/officials/{id}` in `server.rs` + `relay/`, `assets/tl.html`, `assets/tablet.html`, `pages/OfficialsPanel.tsx`, `io/announcer.ts`/`announceCourt.ts`) | `docs/schiedsrichter-management.md` · Spec `docs/features/schiedsrichter-management.md` |
| `pi/` (Raspberry-Pi-Kiosk-Einrichtung) | `docs/pi-setup.md`, `docs/pi-master-image.md` |
| `src-tauri/src/log_upload.rs` | `docs/logging.md` |
| `.github/workflows/*`, Release-Ablauf | `docs/release.md` |
| jede veröffentlichte Version | `docs/changelog.md` |

Offene Punkte / geplante Arbeit → [docs/roadmap.md](docs/roadmap.md).
Große Features bekommen eine **eigene** `docs/*.md` statt einer Sektion in
einer fremden Datei.

**Übergreifend (vor Detail-Dokus lesen, wenn die Aufgabe mehrere
Bausteine berührt):** [`docs/multi-hall.md`](docs/multi-hall.md) — bindet
CourtID-Refactor, Hallen-Gruppierung und LAN+Cloud-Parallelbetrieb zu
einer Architektur-Erzählung.

## Von der Idee zur Spec

Neue Features starten **nicht** mit Code, sondern mit `/idee`: Brief → Grill →
How-To → Spec+Review. Ergebnis ist eine freigegebene Spezifikation unter
`docs/features/<slug>.md` (Zwischenstände gitignoriert unter
`docs/features/_intake/`). Die Kern-Stufen `/grill-me` (Anforderung löchern) und
`/how-to` (Umsetzungsplan entwerfen) sind auch einzeln nutzbar. Bis zur Freigabe
der Spec wird kein Produktivcode geschrieben.

Konzept und Funktionsweise: [docs/spec-pipeline-konzept.md](docs/spec-pipeline-konzept.md).

## Subagents

- **code-reviewer** — nach **jeder** Code-Änderung (Pflicht, in beiden
  Repos badhub + bts-light).
- **security-reviewer** — bei neuem User-Input, Auth, Datei-/URL-Handling.
- **Explore** — breite Recherche im Code.

## Embedded Secrets (bewusst)

BVBB-Push-Token, `BTS_LOG_TOKEN` und der Updater-Signing-Schlüssel sind
**absichtlich** eingebettet — eine Plug-and-play-App ohne Server-Konto
kann keine Geheimnisse zur Laufzeit beziehen. Nicht als Leak behandeln.

## Datenschutz

Kein Geburtsjahr speichern/anzeigen/loggen. Spielernamen nur im Rahmen des
Liveticker-Zwecks. Im Zweifel Feld weglassen.

**Nationalität** (seit 09.08.2026) und **Verein** (seit 12.08.2026) sind
bewusst zuschaltbare, **standardmäßig ausgeschaltete** Anzeige-Felder — beide
stehen ohnehin auf Aushang/Meldeliste. Die **Lizenznummer** reist seit
17.08.2026 im TL-Zustand mit (`team1_ids`/`team2_ids`) — zunächst nur in der
Warteliste, seit 18.08.2026 auch an **laufenden und beendeten** Spielen.
Einziger Zweck an allen drei Stellen: Link auf die badhub-Spielerseite
(`/spieler/<Nr>/live`), deren öffentlicher URL-Schlüssel sie ohnehin ist.
Der Wächter-Test in `tablet/tl.rs`
(`the_state_never_carries_personal_data_beyond_its_purpose`) prüft die drei
Stellen positiv und hält Geburtsjahr, Check-In-Spielernamen sowie
Sperrlisten und Stammverein der Schiedsrichter weiterhin draußen.

---

*Details immer in `docs/`. Diese Datei nur für übergreifende Regeln.*
