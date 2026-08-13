# Turnier-Robustheit — Cluster-Übersicht (Umbrella)

> Status: **abgestimmt 2026-08-13** (via /idee). Ordnet den Cluster ein und
> verweist auf die Detail-Specs. Enthält selbst keine Umsetzung.

## Anlass

Reales 2-Hallen-Turnier: Haupthalle 12 Felder (LAN), Nebenhalle 6 Felder (Cloud,
**eigener Router**), je TVs + Tablets; ~36 Geräte; störanfälliges WLAN mit
hunderten Fremdgeräten. Beim letzten Turnier: „Hänger" in der Software und zu
langsame Score-Anzeige („Klick auf Tablet → TV zeigt Stand erst spät").

Eine belegte Ist-Analyse (Explore, Datei:Zeile) hat die Ursachen lokalisiert;
daraus vier Hebel, priorisiert.

## Die Hebel (Reihenfolge = Priorität)

| # | Hebel | Kern | Detail-Spec |
|---|-------|------|-------------|
| **A** | **Echtzeit-Robustheit der Score-Strecke** | A1 niedrig-latente Anzeige (TVs pollen 1 s/5 s → Push) + A2 Reconnect-Wahrheit (Tablet = Wahrheit, „Slot-Halter gewinnt") | [turnier-robustheit.md](turnier-robustheit.md) ✅ **umgesetzt v0.9.196/197** |
| **B** | **Ergebnis-Weg verlustsicher** | Client/Host-Puffer existieren schon; Rest: `process_result`-Idempotenz (kein Endlos-Retry/Doppel-Write) + Client-Fetch-Timeout + Host-Retry-Queue auf Platte | [ergebnis-puffer.md](ergebnis-puffer.md) ✅ abgestimmt |
| **C** | **Last-/Soak-Test des Relay-Brokers** | In-Process-Concurrency-Harness (multi_thread) gegen die Broker-Eintrittspunkte: beweist Invarianten (Caps, Ownership, Isolation, Cleanup, kein Panic) unter Contention. Socket-Backpressure NICHT abgedeckt (→ manuelle 36-Geräte-Messung). LAN-Server = Folge | [last-soak-test.md](last-soak-test.md) ✅ abgestimmt |
| **D** | **Tote-Verbindungs-Erkennung schärfen** | 17-Min-Fall relay-seitig schon gefixt (A3). Rest: (1) Host-Client Read-Idle (~15s) gegen half-open; (2) `tablet_conn` bekommt eigenen 5-s-Ping + 15-s-Empfangs-Stale (voller `host_conn`-Nachbau). App-Ebene, keine Dep, kein Kill-Switch | [tote-verbindungen.md](tote-verbindungen.md) ✅ **umgesetzt v0.9.199** |

## Abhängigkeiten & Reihenfolge

- **A zuerst** (der sichtbare Schmerz + die Datenintegrität), A1 ist risikoarm und
  sofort auslieferbar, A2 hinter einem Rollback-Schalter.
- **B** ist unabhängig von A und kann parallel/danach.
- **C** sollte kommen, sobald A (und ggf. B) steht — der Last-Test prüft dann das
  reale Verhalten inkl. der neuen Push-/Ownership-Wege.
- **D** ergänzt A2 (beide betreffen Reconnect), baut aber auf keiner A-Änderung auf.

## Gemeinsame Nicht-Ziele (clusterweit)

- Keine TL-Eskalations-UI für Score-Konflikte (Determinismus gewählt).
- Kein Umbau der Court→Match-Zuordnung (bleibt BTP-Wahrheit, R2).
- Keine neue externe Abhängigkeit; keine nginx-Ops-Änderung (WS statt SSE).

## Governance

Jeder Hebel: eigene Detail-Spec über die /idee-Pipeline, TDD (Rust-Unit-Tests),
`code-reviewer` immer, `security-reviewer` bei neuen eingehenden Kanälen/Auth,
Doku laut CLAUDE.md-Tabelle, Version gemeinsam gebumpt. ADRs bei echten
Technik-Entscheidungen (Paket A: ADR 0016 + 0017).
