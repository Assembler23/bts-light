# Last-/Soak-Test des Relay-Brokers (Cluster-Hebel C) — Spezifikation

> Status: **abgestimmt 2026-08-13** (via /idee: Brief → Grill → How-To → Review).
> Quelle: Robustheits-Cluster (nach dem 2-Hallen-Turnier). Betroffene Crates:
> relay (nur Test). ADR:
> [docs/adr/0019-relay-last-soak-inprocess.md](adr/0019-relay-last-soak-inprocess.md).

Teil des Clusters **Turnier-Robustheit**
([Umbrella](turnier-robustheit-cluster.md), **Hebel C**). A (#204/#205) + B
(#206) sind gemergt.

## Kontext / Problem

Die Robustheits-Logik des Relays (Reconnect/Zombie-Host-Ablösung, Ownership
„Slot-Halter gewinnt", Monitor-Nudge-Fan-out, Ergebnis-Idempotenz) ist da, aber
**nie unter Last verifiziert**. Der Relay-Broker ist der geteilte
Serialisierungspunkt: ein **globaler `namespaces`-Mutex** deckt alle Namespaces/
Hallen ab. Es gibt keinen Test, der viele gleichzeitige Geräte, Reconnect-Stürme
oder Ergebnis-Schwälle simuliert — Regressionen (Race, Leak, Cap-Verletzung,
Cross-Namespace-Leck) würden erst im Turnier auffallen.

## Zielbild & Erfolgskriterien

Ein **In-Process-Concurrency-Harness** (`#[cfg(test)] mod load` in
`relay/src/main.rs`), das die Broker-Eintrittspunkte aus vielen gleichzeitigen
`tokio`-Tasks unter **echter Multi-Thread-Contention** treibt:

- **Ehrliche Zielmarke:** beweist **Broker-Invarianten unter Contention + kein
  Panic + Terminierung**, NICHT „robust unter realer Netz-Last" (siehe
  Nicht-Ziele / ADR-Abgrenzung).
- **Leichte Variante** läuft in der CI (`cargo test`, Sekunden) als
  **Regressionswache**; **schwere `#[ignore]`-Soak-Variante** (≥ reales Setup)
  vor dem Turnier manuell (`cargo test -p bts-relay -- --ignored`).
- Findet der Harness ein reales Problem (Race/Leak/Cap-Verletzung), ist es **vor**
  der Halle sichtbar.

## Nicht-Ziele

- **Kein E2E-WS-Socket-Harness** (kein `app()`-Refactor, kein
  `tokio-tungstenite`). Damit **nicht** abgedeckt und im ADR ausdrücklich
  abgegrenzt: **Socket-`send().await`-Backpressure**, **unbegrenztes Wachstum der
  `UnboundedSender`-Queue** bei langsamem Socket, HTTP-Layer, echte
  Scheduling-Reihenfolge. Diese Socket-/Netz-Realität deckt die **manuelle
  36-Geräte-Messung** im echten WLAN ab (Nutzer-Schritt).
- **Keine Behauptung „Deadlock-frei":** der Broker hält den async-`Mutex` nie über
  ein `.await`, Kanäle sind `unbounded` → diese Deadlock-Klasse existiert praktisch
  nicht; die Assertion ist ehrlich „kein Panic + terminiert".
- **LAN-Server** (`src-tauri` `TabletState`, **blockierender** `std::sync::RwLock`
  — laut Grill die schärfere Starvation-Klasse) = eigene Folge-Erweiterung mit
  Risiko-Vermerk, **nicht** dieser Hebel.
- Keine neuen Produktiv-Dependencies; kein Produktionscode-Umbau; keine
  Browser/JS-Last.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crate/Datei:** nur `relay/src/main.rs` — ein neues `#[cfg(test)] mod load`
  hinter `mod tests`. Eintrittspunkte (ohne Sichtbarkeitsänderung erreichbar):
  `register_host`, `attach_tablet`/`take_over_court`, `subscribe_monitor`/
  `unsubscribe_monitor`/`notify_monitor`, `handle_host_frame` (+ `HostFrame::
  ResultAck`), `result`-Handler, `detach_tablet`/`release_host_slot`.
- **Architekturregeln (R1–R6):** Der Harness **prüft** genau die Garantien unter
  Last: **R4** (genau ein aktives Tablet je Court nach Reconnect-Sturm), **R3**
  (Namespace-Isolation, Multi-NS), Cap-Einhaltung. R2/R5 werden nicht neu
  berührt (reiner Broker-Test).
- **Konfiguration:** keine. Keine `config.rs`-Änderung, keine Wire-Änderung.
- **Datenschutz:** keiner (synthetische Geräte-IDs/Court-IDs, keine Personendaten).
- **Abhängigkeiten:** **keine neue** — `tokio::task::JoinSet` (im vorhandenen
  `rt-multi-thread`-Feature) statt `join_all` (`futures` ist NICHT verfügbar).

## Akzeptanzkriterien

Jede Assertion läuft **nach `JoinSet::join_all` über ALLE gespawnten Tasks** und
prüft nur **reihenfolge-unabhängige Invarianten** (Zählungen/Existenz, nie „Gerät
A gewann"). Ein umschließender `tokio::time::timeout` = **Testfehler** bei Ablauf.
Die leichte Variante läuft unter `#[tokio::test(flavor = "multi_thread",
worker_threads = 4)]`.

- [ ] **Massen-Connect:** M Namespaces × F Felder × T Tablets + S Monitor-Subs →
      je NS `tablets.len() == min(F, MAX_TABLETS_PER_NS)`, Subs-Summe
      `== min(S, MAX_MONITOR_SUBS)`, `namespaces.len() <= MAX_NAMESPACES`; **kein
      Cap überschritten**.
- [ ] **Reconnect-Sturm:** je Court T Tasks (`take_over_court`, distinct
      device_id) → **genau ein** `tx` je Court in `tablets` (Identität offen);
      Superseded-Frames **== T-1 je Court**; kein Panic.
- [ ] **Nudge-Fan-out:** court-spezifische + „alle"-Subscriber, N_c Notifies je
      Court → jeder court-c-Sub sah **genau N_c** Nudges (alle `court==c`, 0
      fremd), „alle"-Sub sah `Σ N_c`; **Fremd-Namespace-Sub: 0** (Isolation).
- [ ] **Ergebnis-Schwall:** viele parallele `result`-POSTs (+ idempotente Retries)
      gegen ackende Hosts → am Ende `pending.is_empty()` je NS; jede Antwort
      `ok()` **oder** bekannter Fehlerstring; `MAX_PENDING_PER_NS` nie
      überschritten; kein Panic.
- [ ] **Cleanup:** nach Disconnect aller Geräte sind Sub-Listen `is_empty` und der
      Namespace aus `namespaces` entfernt (kein unbegrenztes Wachsen).
- [ ] **Kein Panic + Terminierung:** alle Szenarien terminieren grün unter dem
      Timeout (leichte + Soak-Variante).
- [ ] **Nicht-flaky:** die leichte Variante läuft **reproduzierbar** grün (mehrere
      Läufe), enthält **keine** timing-/reihenfolgeabhängige Assertion.

## Tests

Der Hebel **ist** die Testsuite. Struktur: geteilter Kern `run_*(p: LoadParams)`
je Szenario; die leichte (CI) und die schwere (`#[ignore]`-Soak) Variante rufen
dieselben `run_*` mit anderen `LoadParams` (keine doppelten Assertionen). Verifikation
vor jedem Commit: `cargo test -p bts-relay` (leicht) grün + mehrfach reproduzierbar;
`cargo clippy -p bts-relay --all-targets -- -D warnings` sauber; die Soak-Variante
manuell einmal grün gezogen.

## Risiken & Rollback

- **Flakiness ist der Killer** (eine flatternde CI-Wache kostet Vertrauen):
  strikt nur Zählungen/Existenz nach `join_all`, nie „welches Gerät", nie
  `seq`-Reihenfolge; die Acker-Task im Ergebnis-Schwall schlank halten (8-s-
  `RESULT_TIMEOUT` nie schlagend); **rx im Test-Hauptscope halten** (nur `tx`
  klonen) — sonst siebt der Broker den Sender aus und die Nullzählung wird falsch;
  `worker_threads=4` fest.
- **Fehl-Lesung „grün = unter Last robust":** durch die ADR-Abgrenzung
  (bewiesen vs. nicht bewiesen) und den Doku-Vermerk entschärft.
- **Rollback:** reiner Test — jederzeit entfernbar, kein Produktions-/Wire-/
  Config-Effekt. Kein App-Bump.

## Offene Fragen / Annahmen

- **Konkrete Zahlen** der leichten Variante werden so gewählt, dass sie in
  Sekunden laufen und dennoch echte Contention erzeugen (multi_thread=4); die
  Soak-Zahlen ≥ reales Setup (z. B. 3 NS × 18 Felder).
- **Annahme:** die Broker-Eintrittspunkte sind aus dem In-File-`mod load`
  erreichbar (bestätigt: `mod tests` ruft sie bereits direkt auf).

## Betroffene Doku-Dateien

- `docs/cloud-relay.md` (Pflicht: In-Process-Last-/Soak-Harness, bewiesen/nicht
  bewiesen, Verweis ADR 0019).
- `docs/regression-suite.md` (der neue Test als CI-Regressionswache + Soak-Aufruf).
- `docs/adr/0019-relay-last-soak-inprocess.md`; `docs/roadmap.md` (Hebel C ✅,
  LAN-Folge notieren).

## Umsetzungs-Hinweise

(Ergebnis der How-To-Phase — Details:
`docs/features/_intake/last-soak-test/3-how-to.md`.)

- Ein `#[cfg(test)] mod load { use super::*; }` in `relay/src/main.rs`; virtuelle
  Geräte = `mpsc::unbounded_channel`, **rx im Hauptscope**, nur `tx` in die Tasks;
  ein `Broker` (Clone) gegen viele `tokio::spawn`; Zählung nach `JoinSet::join_all`
  via `try_recv` + `nudge_of`/`SessionSuperseded`.
- **`tokio::task::JoinSet`** (keine `futures`-Dependency); leichte Variante
  `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]`, Soak zusätzlich
  `#[ignore]`.
- **ADR 0019** vor der Umsetzung finalisieren (Runtime-Flavor + Assertions-Doktrin;
  Grenze bewiesen/nicht-bewiesen).
- **Kein App-Version-Bump** (reiner Relay-Test). **Review:** `code-reviewer`
  (Determinismus/keine timing-Asserts, rx-Pruning-Falle, Cap-Checks);
  `security-reviewer` **nicht** nötig.
