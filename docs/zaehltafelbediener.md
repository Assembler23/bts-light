# Zähltafelbediener (Tabletoperator)

Verwaltung der Zähltafelbediener nach dem Vorbild des Original-BTS
(letilo/bts). Grundlage: [ADR 0007](adr/0007-zaehltafelbediener.md). Wird
**in zwei Phasen** gebaut; hier ist **Phase 1** (rein bts-light-seitig, ohne
neuen BTP-Schreibpfad) beschrieben.

Opt-in: Einstellungen → **„Zähltafelbediener"** → „Warteschlange führen"
(`config.scorekeeper.enabled`, Default aus). Ohne den Schalter ändert sich
nichts.

## Phase 1 — Warteschlange (v0.9.163)

**Idee (wie im Original-BTS):** Der **Verlierer** eines regulär beendeten
Spiels ist als nächster Zähltafelbediener dran. Die Reihenfolge ist eine
globale **FIFO-Warteschlange**.

- **Einreihen:** Der Sync-Loop erkennt beim Feldwechsel ein regulär beendetes
  Spiel (`track_scorekeepers` in `sync.rs`) und reiht bei aktivierter
  Verwaltung den Verlierer ein (`TabletState::enqueue_scorekeeper`).
  **Walkover/Aufgabe/Disqualifikation erzeugen keinen Eintrag** (nur
  `MatchResult::Normal`). Idempotent je Match (Dedup über `enqueued_finishes`),
  Doppel = **ein** Eintrag (das ganze Team).
- **Anzeige & Pflege:** In der **Spielübersicht** listet der Abschnitt
  „Nächste Zähltafelbediener" die Warteschlange (FIFO). Pflege: **vorziehen**
  (`advance_scorekeeper`), **entfernen** (`remove_scorekeeper`), **manuell
  hinzufügen** (`add_scorekeeper`). Die Warteschlange lebt im Arbeitsspeicher
  (nicht persistiert) — ein App-Neustart leert sie.
- **Datenmodell:** `ScorekeeperEntry { key, names, from_court_id, enqueued_ms }`
  in `tablet/state.rs`. `from_court_id` (zuletzt gespieltes Feld) ist für die
  spätere „bevorzugt aufs eigene Feld"-Zuweisung vorgesehen.

Commands: `scorekeeper_queue`, `remove_scorekeeper`, `advance_scorekeeper`,
`add_scorekeeper` (`commands.rs`). Konfiguration:
`config::ScorekeeperConfig { enabled, break_seconds }` (break_seconds Default
300 s, wirkt erst mit der Zuweisung in einer späteren Scheibe).

## Zuweisung beim Feld-Aufruf (Scheibe 2, v0.9.164)

Sobald ein Feld belegt wird, zieht der Sync-Loop einen Bediener aus der
Warteschlange (`assign_scorekeeper_for_court`): **bevorzugt jemanden mit
`from_court_id == court` (spielte zuletzt auf genau diesem Feld — der Verlierer
des Vorspiels), sonst den ältesten** Wartenden. Idempotent je (Feld, Match);
ist die Schlange leer, bleibt das Feld ohne Bediener. Wird das Feld frei oder
wechselt das Spiel, räumt `retain_scorekeeper_assignments` die Zuweisung.

Der zugewiesene Bediener ersetzt in `CourtOverview.scorekeeper` den pro-Feld-
Hinweis (wenn die Verwaltung aktiv ist) und erscheint so in der Spielübersicht
je Feld („Bediener: …").

## Noch offen (nächste Scheiben, Phase 1)

- **Ansage** „Tabletbedienung: {Name}" als Segment der Feld-/
  Vorbereitungs-Ansage + Zweitaufruf-Knopf.
- **Mindestpause** (`break_seconds`) beim Ziehen berücksichtigen.

## Phase 2 (später, eigene Freigabe)

Auscheck des Bedieners in **BTP** (`CheckedIn=false`, Tilos „Schreibweg 2"),
damit BTP ihn nicht parallel für ein eigenes Spiel einplant. Erst nach
echtem-BTP-Gegencheck (Check-in-Bit-Regression v0.9.103), siehe ADR 0007.

## Verwandtes

Der ältere **pro-Feld-Hinweis** (`scorekeeper_by_court`, „Verlierer des
Vorspiels auf diesem Feld" am Tablet, `MatchBrief.scorekeeper`) bleibt
unverändert bestehen; die globale Warteschlange kommt additiv daneben.
