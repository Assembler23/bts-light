# Regressions-Suite — erprobtes Verhalten darf nicht mehr kaputtgehen

Nach dem Zwei-Hallen-Turnier (17.–19.07.2026, v0.9.147: 148/148 Ergebnisse
fehlerfrei) gilt: **Der erprobte Stand ist das schützenswerte Gut.** Neue
Features dürfen bestehendes Verhalten nicht brechen — durchgesetzt wird das
nicht durch Vorsicht, sondern durch Tests.

**Die Regel: Kein Feature-Merge, wenn die Suite rot ist.**

## Durchsetzung (existiert, nichts optional)

- CI-Workflow [`ci.yml`](../.github/workflows/ci.yml) (Pflicht-Check
  `build` der Branch-Protection auf `main`) führt bei jedem PR aus:
  `cargo fmt --check` · `cargo clippy --workspace --all-targets -- -D warnings`
  · **`cargo test --workspace`** · `npm run build` (tsc + vite).
- `main` erlaubt nur Squash-Merges über PRs — kein Weg an der Suite vorbei.
- Lokal spiegeln die Hooks (`.githooks/`, siehe
  [CONTRIBUTING.md](../CONTRIBUTING.md)) fmt und clippy.

## Was die Suite heute garantiert (Stand 20.07.2026, ~240 Tests)

Die turniererprobten Kernpfade und ihre Tests — wer hier etwas ändert,
muss die zugehörigen Tests grün halten oder **bewusst** (im PR begründet)
anpassen:

| Garantie | Tests (Modul, Präfix/Beispiele) |
|---|---|
| **R5-Ergebnis-Validierung**: jedes Tablet-Ergebnis wird geprüft (Match zum Feld, Satzplausibilität, Walkover/Aufgabe-Regeln) | `tablet/server.rs` — `rejects_*`, `result_*`, `match_decided_*`, `process_result_*` (16) |
| **BTP-Schreibpfad** (0.9.147): SENDUPDATE mit Sets/Winner/Duration, Players-Block (LastTimeOnCourt, CheckedIn), CourtID bleibt am beendeten Match, Feld-Freigabe im selben Request | `btp/proto.rs` — `update_request_*`, `court_assign_*`, `courts_update_*` (27) |
| **Tablet-Reconnect** (0.9.147): dasselbe Gerät übernimmt seine Session nahtlos, fremde Geräte sehen „belegt", Frames abgelöster Sessions werden verworfen, leere `deviceId` matcht nie | `relay/main.rs` — `same_device_reconnect_*`, `foreign_device_*`, `superseded_session_*`, `empty_device_id_*` + Host-/Tablet-Routing (13) · `tablet/state.rs` — `claim_court_tracks_holder_device`, `reclaim_supersedes_old_token` |
| **Feld-/Anzeige-Logik am Host**: Court→Match-Auflösung, Live-Score-Vertrauen (auch getrennt), Overview/Monitor, Walkover-Kandidaten, Vorbereitungs-Aufrufe | `tablet/state.rs` (21) |
| **Auto-Feldvergabe**: nur freie/entsperrte Felder, Wartezeit, Spieler-Pause, keine Doppelvergabe, Mehr-Hallen nur mit Aufruf bzw. aktiver Halle | `sync.rs` — `auto_assign_*` (16) |
| **Vergabe-Regeln** (geteilt mit der Turnierleitungs-Oberfläche): Belegt-Begriff, Feld-Erwartung, Sperre, Doppelvergabe, Hallenregel, Spieler-Verfügbarkeit, Reihenfolge, Hallen-Kaskade | `tablet/assign.rs` — `court_occupied_by`, `check_assign`, `check_free`, `PlayerAvailability`, `sort_key`, `hall_for_match` |
| **Datensparsamkeit der Turnierleitungs-Ansicht**: der ausgelieferte Zustand enthält weder Geburtsjahr noch Check-In-Spielernamen, Sperrlisten/Stammverein der Schiedsrichter, Akkustand oder Aufschlag-Anzeige. Die Lizenznummern sind seit 18.08.2026 an allen drei Stellen (Warteliste, laufende, beendete Spiele) **positiv** geprüft. ⚠️ Der Allowlist-Wächter führt eine **flache** Feldnamen-Liste — er schlägt NICHT an, wenn ein bereits erlaubter Feldname in einer weiteren Struktur auftaucht | `tablet/tl.rs` — `every_published_field_is_deliberately_allowed`, `the_state_never_carries_personal_data_beyond_its_purpose` |
| **Mehrbenutzer-Schutz der Feldvergabe**: eine frisch geschriebene Zuweisung blockiert das Feld bis zur BTP-Bestätigung (auch gegen die Automatik), verfällt aber von selbst; eine wiederholte Aktion schreibt nicht doppelt | `tablet/state.rs` — `a_fresh_assignment_reserves_*`, `a_reservation_expires_*`, `a_reservation_is_released_*`, `repeating_an_operation_*` · `sync.rs` — `auto_assign_skips_a_court_someone_just_claimed_by_hand` |
| **Spielzeiten-Auswertung**: vier Achsen aus DENSELBEN Messwerten (Summen gleich), Halle beim Erststempel und immun gegen Hallenwechsel, alter Stand ohne Halle lesbar, Ein-Hallen-Turnier ohne Hallen-Achse — und **die Prognose-Fallback-Kette bleibt unberührt**; die Auswertung wird notfalls zugunsten des Zustands geopfert | `tablet/predict.rs` — `die_*_achse_*`, `die_vier_achsen_zaehlen_dieselben_messwerte`, `die_hallen_achse_aendert_die_prognose_kette_nicht` · `tablet/match_times.rs` — `der_erststempel_haelt_auch_die_halle_fest`, `ein_hallenwechsel_aendert_den_hallenstempel_nicht`, `ein_alter_stand_ohne_halle_bleibt_lesbar` · `sync.rs` — `der_e4_stempel_traegt_die_halle_des_felds` · `tablet/tl.rs` — `das_zeiten_panel_liefert_alle_vier_achsen`, `ein_ein_hallen_turnier_liefert_keine_hallen_achse`, `die_spielzeiten_auswertung_faellt_vor_dem_ganzen_zustand` |
| **Ansage-Aufträge**: hallengenaue Zustellung, Verfall nach 60 s, ehrliche Warnung ohne Ansage-Gerät — und ein **unbekannter Auftragstyp verwirft nie die ganze Charge** (Slave mit älterem Stand im Auto-Update-Fenster; die Spielbeginn-Ansage lässt dabei `call_stages` unberührt) | `tablet/state.rs` — `ein_unbekannter_auftragstyp_verwirft_nicht_die_ganze_charge` · `tablet/tl.rs` — `die_spielbeginn_ansage_*`, `an_announcement_without_a_device_in_the_hall_still_counts_but_says_so` · `scripts/test-start-play-text.mjs` (Wortlaut, CI) |
| **Bediener-Nachruf**: eigener Zähler je Feld (Spieler-Aufrufzahl bleibt stehen), Spielwechsel setzt zurück, Deckel bei 3, ohne zugewiesenen Bediener abgelehnt, Auftrag nur in der Halle des Felds | `tablet/tl.rs` — `der_bediener_nachruf_*`, `ein_neues_spiel_setzt_den_bediener_zaehler_zurueck`, `ein_bediener_nachruf_ohne_zugewiesenen_bediener_wird_abgelehnt` · `scripts/test-scorekeeper-call-text.mjs` (Wortlaut inkl. Stufenwort, CI) |
| **Zähltafelbediener-Übergang + Endezeit-Stempel** | `sync.rs` — `track_scorekeepers_*`, `stamp_finished_*` |
| **Liveticker-Diff/Heartbeat**: erster Push voll, unverändert = nichts, nach Fehler wieder voll | `sync.rs` — `*_plan_*`, `heartbeat_*` · `badhub/diff.rs`, `badhub/payload.rs` (17) |
| **Wire-Kompatibilität App↔Relay**: Serde-Roundtrips aller Frames, `#[serde(default)]`-Abwärtskompatibilität, `merge_device_lists` | `relay-proto/lib.rs` (25) |
| **BTP-Parser**: Snapshot-Parsing inkl. Regressionen echter Turnier-Captures | `btp/model.rs`, `btp/xml.rs`, `btp/wire.rs` (33) · `tests/btp_capture.rs` (echte BTP-Mitschnitte als Fixtures) |
| Court-Monitor-Routen, Slave-Brücke, Sieger-Logik, Config-Migration | `tablet/monitor.rs`, `tablet/slave_bridge.rs`, `tablet/winners.rs`, `config.rs` |
| **Relay-Broker unter Last** (Cluster-Hebel C, ADR 0019): Massen-Connect, Reconnect-Sturm, Nudge-Fan-out, Ergebnis-Schwall, Cleanup — unter echter Multi-Thread-Contention. Bewiesen: Caps, genau ein Halter je Court, Namespace-Isolation, `is_empty`-Cleanup, kein Panic. **Nicht** bewiesen: Socket-Backpressure/Queue-Wachstum (siehe Lücken). | `relay/main.rs` — `#[cfg(test)] mod load` (`run_mass_connect`/`run_reconnect_storm`/`run_nudge_fanout`/`run_result_storm`/`run_cleanup`; leicht in der CI, Soak `#[ignore]`: `cargo test -p bts-relay -- --ignored`) |

## Regeln für jede Änderung

1. **Feature/Fix ⇒ Tests im selben PR.** Ein Bugfix beginnt mit dem
   roten Test, der den Bug nachstellt (Beispiel: die Reconnect-Tests
   aus 0.9.147 entstanden aus dem Turnier-Samstag).
2. **Wire-Typen** (`relay-proto`) nur abwärtskompatibel erweitern:
   neue Felder mit `#[serde(default)]` + Roundtrip-Test. Alte Tablets/
   Relays im Feld reden sonst nicht mehr mit neuen.
3. **Verhaltensänderung an einem garantierten Pfad** = Test-Anpassung
   im selben PR **mit Begründung im PR-Text** — nie Tests löschen, um
   grün zu werden.
4. Echte BTP-Auffälligkeiten als Capture-Fixture in
   `src-tauri/tests/fixtures/` einfrieren (Parser-Regressionen).

## Bekannte Lücken (bewusst, mit Plan)

- **Snapshot-Übernahme in `sync.rs::run_once`**: heute bedingungslos
  (`set_snapshot`), der Leer-Snapshot-Guard (Cluster A) schließt die
  Lücke samt Tests.
- **`assets/tablet.html`** (Vanilla-JS, ~3000 Zeilen): kein vollständiges
  JS-Test-Harness. Absicherung: die Server-Seite validiert jedes Ergebnis
  (R5) und die Rust-Tests decken die Gegenstelle ab; die
  **sicherheitskritische BWF-Aufschlag-Positionierung** (`computeServing`
  + `finalizeSetup`-Paritätstausch + `addPointOnSide`, u. a. für den
  Mid-Game-Einstieg Plan 12b) ist zusätzlich durch den reinen Node-Test
  [`scripts/test-serving.mjs`](../scripts/test-serving.mjs) im CI
  abgesichert (Invariante: Einstieg bei beliebigem Stand == ununterbrochene
  Zählung). Ein volles DOM-Harness ist bewusst zurückgestellt — sonstige
  Änderungen an tablet.html brauchen einen manuellen Test am echten Tablet
  (siehe [tablet.md](tablet.md)).
- **Gong-Auflöse-Timing** (`src/io/announcer.ts`, Plan 15): die reine
  Race-Logik liegt in [`gongTiming.mjs`](../src/io/gongTiming.mjs) und ist
  durch [`scripts/test-gong-timing.mjs`](../scripts/test-gong-timing.mjs)
  im CI abgesichert (onended-Pfad, Fallback-Pfad, done-Guard über einen
  Fake-Timer). Die Web-Audio-Kopplung + die hörbare Wirkung (WebView2-
  Resume-Latenz) bleiben ein manueller Test unter Windows.
- **Satz-/Matchball-Erkennung** (Felderübersicht, Plan 16): die reine Logik
  liegt in [`gamePoint.mjs`](../src/io/gamePoint.mjs) und ist durch
  [`scripts/test-gamepoint.mjs`](../scripts/test-gamepoint.mjs) im CI
  abgesichert (Satzball/Matchball/kein Ball, entschiedener Satz, Cap-Nähe,
  Decider). Rein informativ – ein falsches Badge ist kosmetisch, keine
  Wertung.
- **`run_once`-Gesamtzyklus** (Netz + BTP + badhub zusammen): nur in
  Teilen testbar; die Einzelschritte sind abgedeckt.
- **Socket-Ebene des Relays unter Last** (Cluster-Hebel C, ADR 0019): Der
  In-Process-Last-Harness (`relay/main.rs mod load`) beweist die Broker-
  Invarianten unter echter Contention, aber **nicht** die
  Socket-`send().await`-Backpressure in den `select!`-Loops noch das
  unbegrenzte Wachstum der `UnboundedSender`-Queue bei zähem Socket (der
  Harness leert die Empfänger selbst). Diese reale Netz-Robustheit deckt die
  **manuelle 36-Geräte-Messung** im echten WLAN ab. Ein volles E2E-WS-Harness
  ist bewusst zurückgestellt. Ebenso offen: **LAN-Server-Last**
  (`tablet/state.rs`, blockierender `std::sync::RwLock`) als Folge-Erweiterung.
- **Last der Anzeige-Strecke** (Spec
  [features/monitor-livestand-push.md](features/monitor-livestand-push.md),
  Etappe S0): **manueller Lauf**, kein CI-Schritt — Muster ADR 0019.
  `node scripts/last-monitor.mjs --base http://<turnier-pc>:8088/` fährt
  zwanzig zählende Tablets, zwanzig Feld-Übersichten und wahlweise feste
  Court-Monitore gegen einen **laufenden** Turnier-PC (LAN) bzw. gegen den
  Relay (Cloud) und meldet Abrufe, Bytes und die Latenz Punkt → Anzeige.
  Braucht **belegte Felder**: Ohne gültige Match-ID verwirft `handle_score`
  den Stand, es entstünde weder Schreibvorgang noch Nudge. Vor **und** nach
  jeder Etappe der Spec fahren; die Server-Sicht dazu liefern
  `GET /debug/perf` und die 10-Sekunden-Zeile im Diagnose-Log. Das
  Zählwerk selbst (`tablet/perf.rs`) ist mit elf Unit-Tests abgedeckt,
  darunter ein Wächter gegen Personenbezug im Bericht.

## Abgleich mit Tilos Original-BTS

Tilos Projekt hat ebenfalls eine Suite (Mocha, 14 Testdateien, Travis-CI)
— Prinzip bestätigt. Aufschlussreich sind seine **Blindstellen**: Leerer
BTP-Snapshot (löscht dort ungeprüft alle Matches inkl. laufender),
Reconnect-/`pushall`-Replay und WebSocket-Liveness sind bei ihm weder
abgesichert noch getestet. Genau diese drei Bereiche sind unsere
Cluster-A-Baustellen — wir übernehmen dort **nicht** Tilos Annahmen,
sondern bauen Guard + Tests neu (Details:
[turnier-log-review-2026-07.md](turnier-log-review-2026-07.md),
[btp-write-vergleich-letilo.md](btp-write-vergleich-letilo.md)).
