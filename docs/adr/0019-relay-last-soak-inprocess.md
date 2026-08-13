# 0019 — Relay-Last-/Soak-Test: In-Process, Multi-Thread, ehrliche Abgrenzung

- **Status:** proposed
- **Datum:** 2026-08-13

Gehört zu [docs/features/last-soak-test.md](../features/last-soak-test.md)
(Cluster-Hebel C).

## Kontext

Die Relay-Robustheit ist nie unter Last verifiziert. Ein Test-Harness soll das
absichern — aber die naheliegende Form (`#[tokio::test]` treibt die
Broker-Funktionen) hat zwei Fallen: (1) `#[tokio::test]` ist default
**current_thread** → kooperativ auf einem Thread, **keine** echte Contention am
globalen `namespaces`-Mutex; (2) ein In-Process-Harness, der die mpsc-Empfänger
selbst leert, **entfernt** genau die reale Last-Fehlerklasse (Socket-Backpressure,
unbounded-Queue-Wachstum). Ohne klare Regeln entsteht entweder ein aussageloser
oder ein flaky Test — beides schlimmer als keiner.

## Entscheidung

**(1) Runtime-Flavor + Assertions-Doktrin.** Der Harness läuft als In-File
`#[cfg(test)] mod load` unter `#[tokio::test(flavor = "multi_thread",
worker_threads = 4)]` (fest, damit unabhängig von der Host-Kern-Zahl). Viele
`tokio::spawn`-Tasks treiben die Broker-Eintrittspunkte gegen **einen** `Broker`
(Clone) → echte Contention. **Jede** Endzustands-Assertion läuft **nach
`JoinSet::join_all` über ALLE Tasks** und prüft **nur reihenfolge-unabhängige
Invarianten** (Zählungen/Existenz, nie „welches Gerät gewann", nie
`seq`-Reihenfolge). Ein umschließender `tokio::time::timeout` gilt als
**Testfehler** bei Ablauf (großzügiger Faktor gegen CI-Jitter). Zwei Varianten:
leicht (CI-Pflicht, Sekunden) + schwer (`#[ignore]`-Soak, ≥ reales Setup, manuell)
über denselben `run_*(LoadParams)`-Kern. `tokio::task::JoinSet` statt `join_all`
(`futures` ist nicht verfügbar) → **keine neue Dependency**.

**(2) Ehrliche Abgrenzung bewiesen vs. nicht bewiesen.**
- **Bewiesen:** Cap-Einhaltung (`MAX_TABLETS_PER_NS`/`MAX_MONITOR_SUBS`/
  `MAX_PENDING_PER_NS`/`MAX_NAMESPACES`), Ownership-End-Invariante (genau ein `tx`
  je Court, `T-1` Superseded), Routing/Nudge-Zählung, **Namespace-Isolation** (0
  im Fremd-NS), Cleanup via `is_empty`, „kein Panic + terminiert unter Timeout".
- **NICHT bewiesen (bewusst):** Socket-`send().await`-Backpressure in den
  `select!`-Loops, unbegrenztes Wachstum der `UnboundedSender`-Queue bei zähem
  Socket, HTTP-Layer, echte Scheduling-Reihenfolge. **„Deadlock-Freiheit" wird
  NICHT behauptet** (der Broker hält den async-`Mutex` nie über `.await`, Kanäle
  unbounded → diese Klasse existiert praktisch nicht).

Diese Grenze steht im Test-Doku-Kopf und in `docs/cloud-relay.md`, damit „grün"
nicht als „unter realer Last robust" fehlgelesen wird.

## Alternativen

- **`current_thread`-Runtime:** deterministisch, aber kooperativ auf einem Thread
  → keine Contention, beweist nichts über den geteilten Mutex. **Verworfen.**
- **Volles E2E-WS-Harness** (`app()`-Refactor + `tokio-tungstenite`, `#[ignore]`):
  würde Socket-Backpressure + Queue-Wachstum automatisiert prüfen, kostet aber
  Refactor + Dev-Dep + Flakiness-Risiko. **Out-of-Scope** — die Socket-/Netz-
  Realität deckt die manuelle 36-Geräte-Messung im echten WLAN ab.
- **LAN-Server** (`src-tauri` `TabletState`, blockierender `std::sync::RwLock` —
  über `.await` gehalten die schärfere Starvation-Klasse): **Folge-Erweiterung**
  mit eigenem Risiko-Vermerk, nicht dieser Hebel.

## Konsequenzen

- Eine deterministische CI-Wache fängt Regressionen in Broker-Invarianten (Race/
  Leak/Cap/Isolation/Ownership) unter echter Contention; eine Soak-Variante prüft
  vor dem Turnier ≥ reales Setup.
- **Negativ:** Der Harness beweist NICHT die reale Netz-Robustheit (Socket-Ebene)
  — dafür bleibt die manuelle Messung nötig; die Grenze muss dokumentiert bleiben,
  sonst falsche Sicherheit. Flakiness wäre wertvernichtend → strenge
  Assertions-Doktrin (nur Invarianten nach `join_all`). `worker_threads=4` fest
  (nicht an Kerne gekoppelt).
