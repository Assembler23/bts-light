# 0020 — Tote-Verbindungs-Erkennung: Read-Idle (Option A) + Tablet-Empfangs-Stale

- **Status:** proposed
- **Datum:** 2026-08-13

Gehört zu [docs/features/tote-verbindungen.md](../features/tote-verbindungen.md)
(Cluster-Hebel D).

## Kontext

Der 17-Minuten-Fall ist relay-seitig geschärft (`host_conn`: aktiver 5-s-Ping,
15-s-Empfangs-Stale, `try_claim_host`). Zwei passive Strecken bleiben: der
**Host-Client** (`relay_client.rs`, keine eigenen Pings, kein Read-Idle → stiller
half-open reconnectet erst nach OS-TCP-Timeout) und **`tablet_conn`** (kein
Empfangs-Stale; Relay pingt Tablets nur alle 30 s). Zwei cross-crate-
Entscheidungen sind zu treffen; sie berühren die Grenze relay↔src-tauri und die
Fehlalarm-Balance im flaky WLAN der Nebenhalle.

## Entscheidung

**(1) Host-Client Read-Idle = Option A.** `relay_client.rs` stempelt die letzte
eingehende Nachricht (jeder Frame **oder** der Relay-Ping) und **droppt** die
Verbindung bei `RELAY_READ_IDLE = 15 s` Stille → `run` reconnectet (`connect_async`
öffnet einen frischen Socket; Backoff resettet bei Erfolg). **Kein zusätzlicher
Client-Ping.** Die Schwelle setzt voraus, dass der Relay ≤ 5 s pingt (`HOST_PING`);
das ist ein bewusster, dokumentierter Vertrag mit einer Konstante der unabhängig
deploybaren `relay`-Crate — tragbar, weil beide im selben Repo liegen und
koordiniert deployt werden.

**(2) Tablet-Liveness via eigenen Relay→Tablet-Ping.** `tablet_conn` wird ein
voller `host_conn`-Nachbau: neuer `TABLET_PING = 5 s` (der Relay pingt jedes
Tablet aktiv; der Browser auto-pongt auf **Protokoll-Ebene**) + `TABLET_STALE =
15 s` Empfangs-Stale-`break`, mit der Invariante `TABLET_STALE ≥ 3 · TABLET_PING`.
Der bestehende `detach_tablet`-Slot-Guard (`same_channel`) bleibt (R4). Die
geteilte Konstante `HEARTBEAT` (30 s, auch für `monitor_conn`) bleibt unverändert.

## Alternativen

- **(1) Option B — eigene Client-Pings + Pong-Timeout:** selbstständig (unabhängig
  von der Relay-Ping-Kadenz), aber mehr Code, ein zweiter Ping-Pfad und ein neuer
  expliziter `Message::Ping`→`Pong`-Arm in `host_conn` (heute nur Bibliotheks-
  Auto-Pong). Verworfen — verletzt „kein Umbau der bestehenden Ping/Reconnect-
  Muster"; die Kopplung an `HOST_PING` ist der günstigere, dokumentierte Preis.
- **(2) Verlass auf den App-Ping des Tablets (5 s Text-Ping):** verworfen — mobile
  Browser drosseln `setInterval` im Hintergrund/Sperrbildschirm auf ≥ 1/min → ein
  lebendes, backgroundetes Tablet würde nach 15 s **fälschlich** gedroppt; alte
  Tablet-Seiten garantieren den App-Ping nicht. Der relay-generierte Ping ist
  gegen JS-Drosselung immun (Protokoll-Auto-Pong).
- **TCP-Keepalive / `socket2`:** verworfen — neue Dependency + manueller TCP-Aufbau
  vor dem WS-Upgrade (Feature-Nicht-Ziel).
- **Config-Kill-Switch** (wie `reconnect_legacy_rev` bei A2): verworfen — ein
  Fehl-Drop ist harmlos (Reconnect + Reclaim, Stand persistiert), keine
  Datenverlust-Gefahr; die Schwellen sind konservativ (≥ 3× Ping), wie der
  bestehende LAN-`STALE_AFTER` ohne Schalter.

## Konsequenzen

- Half-open-Host reconnectet in ~15 s statt Minuten; toter Tablet-Slot frei in
  ~15 s statt ~30 s+ — auf beiden Cloud-Strecken, ohne neue Dependency.
- **Negativ / Restrisiken:** Die Read-Idle-Schwelle koppelt an `HOST_PING` (Kommentar
  + dieser ADR halten den Vertrag fest; eine `HOST_PING`-Änderung ist koordiniert
  vorzunehmen). Die Ping-Last je Tablet steigt von 30 s auf 5 s (vernachlässigbar,
  ≤ 30 Felder, wie `host_conn` je Host). Kein Kill-Switch → falls sich die
  Schwellen im Feld doch als zu aggressiv zeigen, ist eine Konstanten-Anpassung +
  Deploy nötig (bewusst, da Fehl-Drops harmlos sind).
