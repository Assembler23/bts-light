# 0038 — Zettel-Ereignisse sind append-only; eine Rücknahme ist ein Eintrag, kein Löschen

- **Status:** proposed
- **Datum:** 2026-08-19

Gehört zu [docs/features/schiedsrichterzettel-druck.md](../features/schiedsrichterzettel-druck.md),
setzt [ADR 0037](0037-zettel-ereignisse-eigener-strom.md) voraus.

## Kontext

Für die Ereignisse hält der **Host** die Wahrheit — sonst kann ein Tablet, das ein Feld
übernimmt, die Karten seines Vorgängers nicht kennen und würde sie beim ersten Sync auslöschen
(genau das tut `TimelineStore::apply_sync` beim Punktverlauf, `timeline.rs:208`: es **ersetzt**).

Zugleich muss der Zettel Korrekturen abbilden können. Am Tablet gibt es „↩" (Undo), und der
Schiedsrichter kann sich schlicht vertippen. Heute ist das kaputt: `snapshot()` sichert
`rallyLog`, **nicht** `STATE.cards` — ein Undo nach einer roten Karte nimmt den Punkt zurück und
lässt die Karte stehen.

Erschwerend ist der Anker instabil: `n` ist keine ID, sondern die **Position** im Satz-String
(`apply_rally` verlangt `n == points.len() + 1`). Nach einem Undo wird dieselbe Nummer neu
vergeben — ein Ereignis „nach Ballwechsel 19" zeigt nach zwei Undos und drei neuen Punkten auf
einen anderen Ballwechsel.

## Entscheidung

**Der Ereignis-Bestand ist append-only. Der Host löscht nie; er lernt Rücknahmen dazu.**

1. **Vereinigung statt Ersetzung.** `SheetStore::apply_event_sync` fügt zusammen: bekannte `id`
   ignorieren, unbekannte anhängen, sortieren nach `(set, after_n, seq)`. Die Operation ist
   **idempotent und kommutativ** — es ist egal, in welcher Reihenfolge Host und Tablet ihre
   Stände lernen, und ein Tablet, das offline war, bringt seine Ereignisse beim Reconnect
   einfach nach. Ein Ersatz-Tablet kann nichts wegnehmen.
2. **Rücknahme ist ein Ereignis.** Wird ein Ereignis ungültig — durch Undo oder Korrektur —,
   entsteht ein neues Ereignis `kind = retract` mit `retracts = <id>`. Nichts wird entfernt.
3. **Der Anker ist eine Schnittposition, keine ID.** `(set, after_n)` plus `score_a`/`score_b`
   als Plausibilität. Nach einem Undo existiert die Position semantisch nicht mehr; die
   betroffenen Ereignisse werden ausdrücklich zurückgenommen, statt sich stillschweigend zu
   verschieben. Am Tablet wandert der Ereignis-Log dafür in `snapshot()`/`restoreSnapshot()`
   mit — nach dem Muster von `rallyLog`.
4. **Der Zettel druckt Zurückgenommenes durchgestrichen in der Protokollzeile, nicht im
   Raster.** Für einen Archivbeleg ist das ehrlicher als spurloses Verschwinden.

## Alternativen

- **Löschen beim Sync** (Tablet ist autoritativ, wie beim Punktverlauf). Verworfen: Ein
  Ersatz-Tablet mitten im Spiel kennt die Ereignisse des Vorgängers nicht. Sie müssten dafür im
  `state_sync` mitreisen — und der liegt als `court_state` im **RAM des Relays auf badhub.de**
  und geht per `StateRestore` an jedes Gerät, das den Court-Slot übernimmt. Sanktionsdaten
  gehören dort nicht hin.
- **Stabile Ereignis-IDs am Ballwechsel** (jeder Ballwechsel bekommt eine unveränderliche ID).
  Verworfen: Das änderte das Punktverlauf-Format und damit die Byte-Gleichheit der Graph-Sicht —
  ein hoher Preis für einen Anker, den die Schnittposition genauso trägt.
- **Ereignisse hart löschen statt zurücknehmen.** Verworfen: Ohne Löschweg über den Sync bräuchte
  es einen eigenen Lösch-Frame, und ein Archivbeleg, aus dem Einträge spurlos verschwinden
  können, ist weniger wert als einer, der Korrekturen zeigt.

## Folgen

- Der Bestand wächst monoton; `MAX_EVENTS_PER_MATCH = 64` zählt **auch die Rücknahmen** mit.
  Bei realistisch ≈ 20 Ereignissen je Match ist der Abstand groß genug.
- Der Zettel zeigt Korrekturen offen — gewollt für einen Archivbeleg, aber Bediener müssen
  wissen, dass eine versehentliche Karte sichtbar bleibt (in `docs/schiedsrichterzettel.md`
  zu erklären).
- Die Rücknahme-Logik lebt in `src/io/matchEvents.mjs` als reine Funktion und wird dort
  getestet (`undoSchnitt`, `vereinigen`), nicht nur im Asset.
- Ereignisse sind damit unabhängig von der Reihenfolge, in der Frames eintreffen — ein
  Robustheitsgewinn gegenüber dem Punktverlauf, der bei einer Lücke einfriert, bis ein
  `rally_sync` kommt.
