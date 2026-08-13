# Tote-Verbindungs-Erkennung schärfen (Cluster-Hebel D) — Spezifikation

> Status: **abgestimmt 2026-08-13** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Robustheits-Cluster (2-Hallen-Turnier, eigener Router in der Nebenhalle).
> Betroffene Crates: relay + src-tauri. ADR:
> [docs/adr/0020-tote-verbindung-read-idle-tablet-stale.md](adr/0020-tote-verbindung-read-idle-tablet-stale.md).

Teil des Clusters **Turnier-Robustheit**
([Umbrella](turnier-robustheit-cluster.md), **Hebel D**). A (#204/#205), B (#206),
C (#207/#208) sind gemergt.

## Kontext / Problem

Der 17-Minuten-Fall (tote Host-TCP-Verbindung hielt den Slot bis zum OS-Timeout)
ist **relay-seitig bereits geschärft** (Cluster A3: `host_conn` pingt alle 5 s,
droppt bei 15 s Stille, `try_claim_host` löst einen stummen Host ab). Es bleiben
**zwei passive Strecken**, auf denen ein toter Peer erst spät auffällt:

1. **Host-Client half-open** (`relay_client.rs`): sendet keine eigenen Pings, hat
   kein Read-Idle-Timeout; die ausgehenden Ticker senden nur bei Änderung
   (dedupliziert). Bei half-open TCP (Netz weg, kein RST) reconnectet ein stiller
   Master erst nach OS-TCP-Timeout (Minuten) — der Relay hat seinen Slot da längst
   freigegeben, aber niemand claimt neu.
2. **Tablet↔Relay passiv** (`tablet_conn`): kein Empfangs-Stale (anders als
   `host_conn`); der Relay pingt Tablets nur alle 30 s → ein totes Tablet belegt
   den Relay-Slot bis zum 30-s-Ping-Sendefehler.

## Zielbild & Erfolgskriterien

Tote/half-open Verbindungen auf beiden Strecken **proaktiv** erkennen, App-seitig,
**ohne neue Dependency**, am bewährten `host_conn`-Muster:

- Eine **half-open Host-Verbindung** wird client-seitig in **~15 s** erkannt →
  Reconnect → neuer Claim → Host-Slot frei (statt Minuten).
- Der **Relay-Slot eines toten Tablets** wird in **~15 s** frei (wie beim
  LAN-Server) statt bei ~30 s+.
- **Kein Fehl-Drop gesunder Tablets** — auch nicht backgroundeter (mobile Browser
  drosseln JS-Timer): der Relay pingt aktiv, der Browser auto-pongt auf
  Protokoll-Ebene.
- **Kein Churn** bei 5–10-s-WLAN-Hängern (Schwellen ≥ 3× Ping-Intervall).

## Nicht-Ziele

- **TCP-Keepalive / `socket2`** (neue Dependency + manueller TCP-Aufbau vor dem
  WS-Upgrade).
- Umbau des bereits guten `host_conn` (Relay→Host) und des **LAN-Servers**
  (Tablet↔Server, schon aktiv mit `STALE_AFTER=10s`).
- **`monitor_conn`** (Poll/Anzeige, unkritisch) und der Relay-Listener-Socket
  (127.0.0.1 hinter Proxy). Änderung von `HEARTBEAT` (mit `monitor_conn` geteilt)
  oder `HOST_STALE`/`STALE_AFTER`.
- **Config-Kill-Switch** (ein Fehl-Drop ist harmlos — Reconnect + Reclaim, Stand
  persistiert; keine Datenverlust-Gefahr wie bei A2).
- **Eigene Client-Pings** (Option B) — Read-Idle nutzt den bestehenden Relay-Ping
  (Option A, ADR 0020).

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - Teil (2): `relay/src/main.rs` `tablet_conn` — neuer `TABLET_PING`-Ticker (5 s)
    + `TABLET_STALE` (15 s) Empfangs-Stale-`break`, `last_incoming` vor dem inneren
    `match` gestempelt. Der bestehende Slot-Guard (`detach_tablet`/`same_channel`)
    und die Konstante `HEARTBEAT` (für `monitor_conn`) bleiben unverändert.
  - Teil (1): `src-tauri/src/tablet/relay_client.rs` `serve`-Loop — `RELAY_READ_IDLE`
    (15 s) + Idle-Ticker; `last_incoming` bei jedem eingehenden Frame/Ping; bei
    Stille `return Err` → bestehender `run`-Reconnect.
- **Architekturregeln (R1–R6):**
  - **R3:** betrifft **beide** Cloud-Strecken (Host↔Relay, Tablet↔Relay); LAN
    unverändert.
  - **R4:** ein schnell freigegebener Tablet-Slot verletzt „ein aktives Tablet je
    Court" nicht — der `same_channel`-Slot-Guard verhindert, dass eine abgelöste
    Alt-Verbindung dem Reclaim-Gerät den Slot wegräumt; ein passives Wartetablet
    wird nicht auto-promoviert.
  - R2/R5/R6 unberührt.
- **Konfiguration & Abwärtskompatibilität:** **keine** neuen Config-Felder (nur
  Konstanten). Kein Wire-/Schema-Effekt. Auto-Update-sicher. Beide Binaries
  (App + Relay) werden versioniert; die Read-Idle-Kopplung (Option A) setzt
  voraus, dass der Relay mit `HOST_PING ≤ 5 s` (unverändert) läuft.
- **Datenschutz:** keiner. **Abhängigkeiten:** **keine neue** (App-Ebene, tokio
  `interval`/`Instant` vorhanden).

## Akzeptanzkriterien

- [ ] **`TABLET_STALE ≥ 3 · TABLET_PING`** hält (Invariante-Test, analog dem
      bestehenden `HOST_STALE`-Test). Werte: `TABLET_PING = 5 s`,
      `TABLET_STALE = 15 s`.
- [ ] **Stale-Entscheidung (rein):** `is_stale(last, now, threshold)` liefert
      `false` bei `now = last + (threshold − ε)` und `true` bei `now = last +
      threshold` (Grenze `>=`).
- [ ] **Kein Fehl-Drop:** ein Tablet/Host, dessen `last_incoming` regelmäßig (je
      Ping/Pong) aufgefrischt wird, wird **nie** gedroppt (die reine Entscheidung
      bleibt `false`).
- [ ] **`tablet_conn`** pingt aktiv alle `TABLET_PING` und bricht bei
      `TABLET_STALE` Empfangs-Stille ab; der bestehende Slot-Guard bleibt (eine per
      Reclaim abgelöste Alt-Verbindung räumt dem neuen Tablet nichts weg — R4).
- [ ] **`relay_client`** bricht die Verbindung bei `RELAY_READ_IDLE` Empfangs-
      Stille ab und reconnectet (Backoff resettet bei Erfolg).
- [ ] **`monitor_conn`, `host_conn`, LAN-`STALE_AFTER`, `HEARTBEAT`** unverändert.

## Tests

**Rust-Unit-Tests (TDD):**
- Reine `is_stale`-Grenzfälle (unter/an der Schwelle) — in beiden Crates.
- Invariante `TABLET_STALE >= TABLET_PING * 3` (relay).
- „Gesundes Tablet wird nicht gedroppt" als reine-Entscheidungslogik (frischer
  `last`).
- **R4-Regression:** der bestehende `same_device_reconnect_replaces_old_session`-
  Test bleibt grün (schneller Stale-Drop bricht ihn nicht).

Der Loop selbst wird — wie beim `host_conn`-Präzedenzfall — **nicht** mit
gefälschten Sockets getestet (Kosten/Nutzen); die Entscheidung ist über die reine
Funktion abgedeckt. `cargo test` grün, `cargo clippy --workspace --all-targets -D
warnings` sauber vor jedem Commit.

## Risiken & Rollback

- **Fehl-Drop bei WLAN-Hänger:** 5–10-s-Hänger < 15 s → kein Drop. Ein echter
  Fehl-Drop ist **harmlos**: sofortiger Reconnect, Stand persistiert, `same_device`-
  Reclaim; kein Datenverlust → **kein Kill-Switch** nötig.
- **Ping-Last** (5 s statt 30 s an alle Tablets): ein 2-Byte-Ping/5 s an ≤~30
  Felder ist vernachlässigbar (`host_conn` fährt denselben Takt je Host). Kein
  neuer Input/Route/Auth → kein neuer Angriffsvektor.
- **Rollback:** reine Verhaltens-Verschärfung, jederzeit reversibel (Konstanten/
  Zeilen). App-Downgrade + Relay-Rollback = Status quo ante. Kein Schema-/Config-
  Zwang.

## Offene Fragen / Annahmen

- **Kopplung (Option A):** die `relay_client`-Schwelle (15 s) setzt Relay-
  `HOST_PING ≤ 5 s` voraus; im Kommentar + ADR festgehalten (Mono-Repo). Eine
  künftige `HOST_PING`-Änderung ist eine koordinierte Änderung.
- **Annahme:** der `detach_tablet`-Slot-Guard (`same_channel`) genügt für R4 beim
  schnelleren Stale-Drop (bestätigt in der Kartierung).

## Betroffene Doku-Dateien

- `docs/cloud-relay.md` („Zombie-Host-Ablösung"): Tablet↔Relay-Absatz
  (`TABLET_PING`/`TABLET_STALE`) + Client-Read-Idle-Absatz (Kopplungshinweis).
- `docs/adr/0020-tote-verbindung-read-idle-tablet-stale.md`; `docs/roadmap.md`
  (Hebel D ✅); je Version `docs/changelog.md`.

## Umsetzungs-Hinweise

(Ergebnis der How-To-Phase — Details:
`docs/features/_intake/tote-verbindungen/3-how-to.md`.)

- **Teil (2)** `tablet_conn` als `host_conn`-Nachbau: `TABLET_PING`/`TABLET_STALE`
  bei den Host-Konstanten; `interval(HEARTBEAT)` → `interval(TABLET_PING)`;
  `last_incoming` vor dem `match` stempeln; Stale-`break` im Ping-Tick-Arm; Cleanup
  + `HEARTBEAT` unverändert. Kein Pong-Arm.
- **Teil (1)** `relay_client`: `RELAY_READ_IDLE` + Idle-Ticker (~5 s),
  `last_incoming`-Stempel im read-Arm, `return Err` bei Stille. Kein Client-Ping.
- **Reine `is_stale`-Funktion** je Crate für die Tests; Invariante-Test; kein
  Socket-Fake-Loop-Test.
- **ADR 0020** vor der Umsetzung finalisieren (Option A; Tablet-Ping-Liveness).
- **Version:** App-Bump (`src-tauri/Cargo.toml` + `tauri.conf.json` +
  `package.json`) **und** Relay-Bump (`relay/Cargo.toml`) — verschiedene Binaries.
- **Review:** `code-reviewer` (beide Pfade); `security-reviewer` nicht erzwungen
  (kein neuer Input/Auth) — optionaler Blick auf den 5-s-Ping-Last-Aspekt.
