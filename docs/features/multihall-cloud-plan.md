# Mehr-Hallen über Cloud (Weg B): ferne Halle aus dem Master speisen

> **Status: freigegebener Plan (zur Prüfung), Umsetzung beginnt mit B1a.**
> Ergänzt `docs/multi-hall.md` (LAN-Phasen 1/1b/2 sind ausgeliefert).

## Context
Zwei Hallen km entfernt, **getrennte LTE-Netze** (kein gemeinsames LAN), nur **ein** Windows-PC
mit BTP. Ziel: **Master liest BTP → über bts-light an den Slave in Halle B → dort über bts-light
steuern.** Damit ist **Weg B** gesetzt: nur EIN BTP-Client (Master), die Frage „verträgt BTP mehrere
Clients?" entfällt, funktioniert ohne BTP-Fernzugriff.

**Stand heute (im Code geprüft):** Der Cloud-Relay trägt **nur Court-Monitore + Tablets** (pro Feld
`MatchBrief`/Score/Labels) + Monitor-Config/Ads. **LAN-only:** Info-Monitore (Übersicht/Vorbereitung/
Sieger), Kombi-Monitore, **Ansage** (Freitext-Slave holt per `http://<master>:8088/...`). TODO in
`relay_client.rs:228`; Cloud-Monitor-Zuweisungen nur CourtID (`MonitorControl.assignments:
HashMap<String,i64>`). **Court-Monitor (1 Feld) in Halle B geht über Cloud bereits.**

## Entscheidung
**Weg B, phasiert.** Master = alleiniger BTP-Client + Cloud-Host; ferne Halle = **Cloud-Slave**, der
seine Daten aus dem Relay zieht (statt BTP). Start **B1** (sieht & hört), dann **B2** (steuert).

## Pairing (für alle Phasen)
Der Slave braucht den **Namespace des Masters** (`install_id`). Master zeigt einen **Kopplungs-Code**;
Slave-Feld `master_namespace` (+ `announce_hall`). Nutzt das vorhandene Namespace-/`install_id`-Modell.

## B1 — Ferne Halle „sieht & hört" (read-only)
### B1a — Cloud-Ansage (zuerst)
- Master pusht **Freitext** (neu `HostFrame::Freetext{id,hall,text}`) + nutzt die schon vorhandenen
  Per-Court-`MatchAssigned`-Frames für die Auto-Feld-Ansage.
- Relay: `Namespace.freetext` + Route `GET /{ns}/info/announce/freetext?hall=&since=` (+ Hallen-Court-State).
- **Cloud-Ansage-Slave (neu):** `connection_mode=cloud` + `slave_mode` + `master_namespace`; pollt
  Relay statt BTP und sagt lokal an (wiederverwendet `io/announceCourt.ts`, `playFreeText`).
### B1b — Cloud-Info-/Kombi-Monitore (alter „Phase 3")
- `MonitorControl` trägt vollen `MonitorTarget` (behebt `relay_client.rs:228`).
- Master pusht hallen-gefilterten Overview-/Preparation-/Winners-Snapshot; Relay-Routen
  `GET /{ns}/info/overview/state?hall=` usw.; Info-HTML lesen im Cloud-Fall vom Relay.

## B2 — Ferne Halle „steuert" (bidirektional, groß)
- Slave→Master-Befehle über Relay (Feldvergabe/Vorbereitung/Ergebnis) → Master schreibt nach BTP
  (vorhandene `assign_court`/`free_court`/SENDUPDATE); Slave-Sperre durch „an Master senden" ersetzen.
- Hall-B-Tablets verbinden mit dem Slave (lokales LAN); Scores/Ergebnisse Slave→Relay→Master→BTP
  (Muster: Relay `pending`/`oneshot`, Slave→Master-Richtung).

## Betroffene Dateien
- `relay-proto/src/lib.rs`, `src-tauri/src/tablet/relay_client.rs`, `relay/src/main.rs`,
  `src-tauri/src/tablet/{server.rs,state.rs}`, `src-tauri/src/config.rs`, `src/types.ts`,
  Frontend (Cloud-Slave + Pairing-UI), `docs/multi-hall.md`, `docs/changelog.md`.

## Verification
- `cargo fmt/clippy/test`, `npm run build`, HTML-JS via node-`vm`; code-reviewer + security-reviewer
  (neue Relay-Routen). Manuell über `test.badhub.de` mit 2 Profilen/Netzen. Auslieferung: Version-Bump,
  PR→Build→Merge→Tag, Auto-Update; **Relay-Redeploy** (badhub) bei Relay-Änderungen.

## Erster Schritt
**B1a (Cloud-Ansage)** — kleinster eigenständiger Nutzen, klärt das Pairing einmal für alle Phasen.
