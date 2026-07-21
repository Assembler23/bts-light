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
je Feld („Bediener: …"). Wird die Verwaltung mitten im Turnier **abgeschaltet**,
löscht der Sync-Loop alle Zuweisungen (`clear_scorekeeper_assignments`) — es
bleibt kein veralteter Name in der Anzeige hängen; angezeigt wird dann wieder
der pro-Feld-Hinweis. Bei mehreren gleichzeitig neu belegten Feldern wird
nach CourtID sortiert zugewiesen (deterministisch/fair).

## Ansage (Scheibe 3, v0.9.165)

Ist ein Bediener zugewiesen (`CourtOverview.scorekeeper_assigned == true`),
hängt die Feld-Ansage am Ende „**Tabletbedienung: {Name}.**" an (EN:
„Scoreboard operator: …"). Umgesetzt in `announcer.ts`
(`buildAnnouncementSegments` + `buildAnnouncementSsml`, Feld
`scorekeeperNames`); die Auslöser (`announceCourt`, `MatchAnnouncer`) geben die
Namen **nur** weiter, wenn `scorekeeper_assigned` — der reine pro-Feld-Hinweis
wird nicht angesagt. Gilt für Standard- und Azure-Stimme.

## Noch offen

- **Cloud-Slave-Ansage (bekannte Grenze):** Warteschlange und Zuweisung leben
  **auf dem Master** (Sync-Loop). Ein Cloud-Ansage-Slave einer fernen Halle
  sagt seine Court-Matches an, kennt aber die Bediener-Zuweisung nicht — er
  sagt „Tabletbedienung: …" daher **nicht** mit. Nur die Master-Ansage
  (LAN-`MatchAnnouncer`) nennt den Bediener. Fix (später): den zugewiesenen
  Bediener je Feld über den Relay an die ferne Halle pushen (analog
  `AnnounceState.prepared`).
- **Mindestpause** (`break_seconds`): in Phase 1 **ohne Wirkung** — ein
  Bediener verlässt beim Zuweisen die Warteschlange und wird nicht automatisch
  wieder eingereiht, eine „Pause nach dem Dienst" hat hier also keinen Effekt.
  Die Pause greift erst mit **Phase 2** (BTP-Auscheck mit künstlich
  verschobenem `last_time_on_court`), damit BTP den Bediener nicht zu früh für
  ein eigenes Spiel einplant. Das Config-Feld ist bereits vorhanden.
- Optional: ein Zweitaufruf-Knopf „… bitte als Tabletbedienung melden".

## Phase 2 (später, eigene Freigabe)

Auscheck des Bedieners in **BTP** (`CheckedIn=false`, Tilos „Schreibweg 2"),
damit BTP ihn nicht parallel für ein eigenes Spiel einplant. Erst nach
echtem-BTP-Gegencheck (Check-in-Bit-Regression v0.9.103), siehe ADR 0007.

## Verwandtes

Der ältere **pro-Feld-Hinweis** (`scorekeeper_by_court`, „Verlierer des
Vorspiels auf diesem Feld" am Tablet, `MatchBrief.scorekeeper`) bleibt
unverändert bestehen; die globale Warteschlange kommt additiv daneben.
