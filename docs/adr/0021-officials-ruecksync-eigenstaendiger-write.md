# 0021 — Officials-Rücksync: eigenständiger Write, BTP gewinnt

- **Status:** accepted
- **Datum:** 2026-08-13

## Kontext

Die Spec [Schiedsrichtermanagement](../features/schiedsrichter-management.md)
braucht einen Weg, SR/AR-Zuweisungen aus BTS Light nach BTP zu schreiben
(`Match.Official1ID`/`Official2ID`). Vorbelastung: Das Original-BTS
(letilo) schrieb Officials nur eingebettet ins Match-Update beim Ruf aufs
Feld; ein eigenständiger Schreibweg war unbelegt. Zudem der Präzedenzfall
`LocationID` (Messung 10.08.2026): BTP quittiert den Write mit `Result=1`
und **verwirft den Wert still** — jede Schreib-Hypothese muss deshalb
durch Zurücklesen bewiesen werden.

**Messung 13.08.2026** am echten BTP (Testturnier, 3 Officials,
`btp_officials_probe.rs`):

- **V1 eigenständig** (`Match{ID, DrawID, PlanningID, Official1ID,
  Official2ID}`, ohne `Status`): **angenommen** — Werte stehen nach ≤1 s
  im nächsten `SENDTOURNAMENTINFO`.
- **V2 eingebettet** in die Feldzuweisungs-Form (zusätzlich `CourtID` +
  `Courts`-Block): ebenfalls **angenommen**.
- Löschen per Wert `0` funktioniert; keine Nebenwirkungen an
  Sets/Winner/Status; die Übernahme ist **asynchron** (sofortiges
  Zurücklesen zeigt noch den alten Stand).
- Semantik bestätigt: `Official1ID` = Schiedsrichter, `Official2ID` =
  Aufschlagrichter.

## Entscheidung

1. **Jede** SR/AR-Zuweisungsänderung in BTS Light wird per
   **eigenständigem** Match-Update nach BTP geschrieben (V1) — auch
   nachträgliche Änderungen und Änderungen an laufenden Spielen. Beim Ruf
   aufs Feld wandern die Officials zusätzlich mit ins bestehende
   Zuweisungs-Update (V2), damit ein einziger Request genügt.
2. Der Request trägt **nie** ein `Status`-Feld (Check-in-Bitfeld,
   Regression v0.9.103) und folgt dem Reconcile-Muster der Highlights
   (`sync.rs`): gewünschter Stand → Diff → schreiben → Stand nur bei `Ok`
   übernehmen.
3. **Konfliktregel (R2): BTP gewinnt.** Trägt der Snapshot am Match
   `Official1ID`/`Official2ID`, gilt dieser Wert; lokale Zuweisungen sind
   nur ein Overlay für den kurzen Moment bis zur Snapshot-Bestätigung
   bzw. für den Fehlerfall des Writes. Wegen der gemessenen asynchronen
   Übernahme gilt: Ein frisch geschriebener Wert wird bis zur Bestätigung
   im Snapshot lokal angezeigt (kein Flackern), danach ist der Snapshot
   maßgeblich.

## Alternativen

- **Nur beim Feld-Aufruf schreiben (wie letilo):** war die konservative
  Vorentscheidung der Spec, solange der eigenständige Weg unbewiesen war.
  Durch die Messung überholt — verworfen, weil nachträgliche Änderungen
  sonst dauerhaft zwei Wahrheiten erzeugen.
- **Kein Rücksync (reines Overlay):** nur noch Fallback, falls sich der
  Write auf einer anderen BTP-Version anders verhält; die Anzeige-Kette
  funktioniert auch ohne Rücksync.

## Konsequenzen

- BTP und BTS Light bleiben bei Officials deckungsgleich; die
  BTP-Oberfläche zeigt jede Zuweisung aus BTS Light.
- Der Sync-Loop braucht ein Officials-Reconcile mit Retry (Muster
  `reconcile_highlights`); Fehlversuche dürfen die Zuweisungs-Anzeige
  nicht zurückwerfen.
- Die asynchrone Übernahme (≤1 s gemessen) verlangt Toleranz beim
  Vergleich „geschrieben vs. Snapshot" — kein sofortiges Zurückfallen auf
  den alten Snapshot-Wert direkt nach einem Write.
- Gemessen wurde **eine** BTP-Version an **einem** Turnier; die Probe
  (`btp_officials_probe.rs`, `#[ignore]`) bleibt im Repo, um andere
  BTP-Stände schnell gegenzuprüfen.
