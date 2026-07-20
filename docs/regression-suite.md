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
| **Zähltafelbediener-Übergang + Endezeit-Stempel** | `sync.rs` — `track_scorekeepers_*`, `stamp_finished_*` |
| **Liveticker-Diff/Heartbeat**: erster Push voll, unverändert = nichts, nach Fehler wieder voll | `sync.rs` — `*_plan_*`, `heartbeat_*` · `badhub/diff.rs`, `badhub/payload.rs` (17) |
| **Wire-Kompatibilität App↔Relay**: Serde-Roundtrips aller Frames, `#[serde(default)]`-Abwärtskompatibilität, `merge_device_lists` | `relay-proto/lib.rs` (25) |
| **BTP-Parser**: Snapshot-Parsing inkl. Regressionen echter Turnier-Captures | `btp/model.rs`, `btp/xml.rs`, `btp/wire.rs` (33) · `tests/btp_capture.rs` (echte BTP-Mitschnitte als Fixtures) |
| Court-Monitor-Routen, Slave-Brücke, Sieger-Logik, Config-Migration | `tablet/monitor.rs`, `tablet/slave_bridge.rs`, `tablet/winners.rs`, `config.rs` |

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
- **`assets/tablet.html`** (Vanilla-JS, ~3000 Zeilen): kein JS-Test-
  Harness. Absicherung heute: die Server-Seite validiert jedes Ergebnis
  (R5) und die Rust-Tests decken die Gegenstelle ab. Ein Harness ist
  bewusst zurückgestellt — Änderungen dort brauchen einen manuellen
  Test am echten Tablet (siehe [tablet.md](tablet.md)).
- **`run_once`-Gesamtzyklus** (Netz + BTP + badhub zusammen): nur in
  Teilen testbar; die Einzelschritte sind abgedeckt.

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
