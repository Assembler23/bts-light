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
| Sprachansagen (`io/announcer.ts`, `components/MatchAnnouncer.tsx`, `Discipline`) | `docs/announcements.md` |
| Court-Monitor (`tablet/monitor.rs`, `assets/monitor.html`, `assets/flags/`, Court-/Monitor-Routen in `server.rs` + `relay/`) | `docs/court-monitor.md` |
| `src-tauri/src/log_upload.rs` | `docs/logging.md` |
| `.github/workflows/*`, Release-Ablauf | `docs/release.md` |
| jede veröffentlichte Version | `docs/changelog.md` |

Offene Punkte / geplante Arbeit → [docs/roadmap.md](docs/roadmap.md).
Große Features bekommen eine **eigene** `docs/*.md` statt einer Sektion in
einer fremden Datei.

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

---

*Details immer in `docs/`. Diese Datei nur für übergreifende Regeln.*
