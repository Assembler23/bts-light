# 0065 — „Mannschaften berichtigt" als Kipp-Ereignis; Tablet-Korrektur als eigener Undo-Schritt

- **Status:** accepted (Spec freigegeben 30.10.2026, Umsetzung offen)
- **Datum:** 2026-10-30

Gehört zu [docs/features/tablet-seiten-korrigieren.md](../features/tablet-seiten-korrigieren.md),
erweitert [ADR 0038](0038-ereignisse-append-only.md).

## Kontext

Am Zähl-Tablet soll man nachträglich korrigieren können, dass die falsche Mannschaft als
„links" aufgestellt wurde. Die Punkte gehören dann zur Seite, nur die Namen sind vertauscht:
Jeder bisher gemeldete Stand und jedes Zettel-Ereignis mit Team-Bezug (`serve_start`,
Karten, `scoreA`/`scoreB`) steht in falschen A/B-Koordinaten.

Zwänge:
- Der Ereignisbestand ist append-only, der Host **vereinigt** nur (ADR 0038, `sheet.rs`).
- Je Spiel gelten höchstens 64 Ereignisse einschließlich Rücknahmen (`MAX_EVENTS_PER_MATCH`).
- `EventKind` hat bewusst keinen Catch-All: Eine unbekannte Art lässt den ganzen Frame scheitern.
- Undo am Tablet läuft über Snapshots je Punkt, die auch `teamOnSide`/`positions`/`rallyLog`
  enthalten. Eine Korrektur ohne eigenen Schritt würde beim Undo eines älteren Punkts still
  wieder verschwinden.
- badhub nimmt im Verlauf nur `serve_start` an (`Verlauf.php`).

## Entscheidung

1. **Neues Ereignis `teams_corrected`** (`EventKind::TeamsCorrected`). Die Deutung liegt
   allein beim Host (`in_endkoordinaten`, genutzt von Zettel und badhub-Verlauf):
   - Kipp-Punkte sind jedes `teams_corrected` und jede erste Rücknahme eines solchen.
   - Ein Ereignis wird A↔B gekippt, wenn nach ihm eine ungerade Zahl von Kipp-Punkten liegt
     (Ordnung `(set, afterN, seq, id)`).
   - Den Punktverlauf deutet niemand: Das Tablet kippt `rallyLog` selbst und sendet `rally_sync`.
2. **Korrektur = eigener `history`-Schritt** mit Marker am Snapshot; „zurück" nimmt zuerst
   ihn zurück, über Snapshot-Wiederherstellung plus Rücknahme-Ereignis (ADR 0038).
3. **Anker im beendeten Spiel** ist der letzte gespielte Satz, nie Satz n+1.
4. **Deckel-Prüfung vorab:** Reicht der Platz nicht, wird die Korrektur verweigert, nie ohne
   Ereignis ausgeführt.
5. **Kompatibilitäts-Schalter** `MatchBrief.teamsCorrection`: Nur ein Host, der die Art kennt,
   schaltet die Korrektur am Tablet frei.

## Alternativen

- **Alle Ereignisse zurücknehmen und gekippt neu ausstellen** (ohne Protokolländerung): sprengt
  den Deckel (+2 Einträge je Ereignis und Korrektur), das Protokoll wird unlesbar, und
  neu ausgestellte Ereignisse brauchen alte Anker, die `meUndoSchnitt` nicht erfasst. Verworfen.
- **Zettel unkorrigiert lassen, nur Hinweis drucken:** Archivbeleg und badhub-Verlauf wären
  falsch. Verworfen.
- **Korrektur als Gegenoperation ohne Undo-Schritt** (beide Eingriffe sind Involutionen):
  Im Doppel nicht umkehrbar, weil die alte Aufstellung nach der Neuabfrage weg ist; das
  Undo eines älteren Punkts höbe die Korrektur still auf. Verworfen.
- **Rücknahme zählt nicht als Kipp-Punkt** (nur nicht zurückgenommene Korrekturen kippen):
  Eine Karte, die zwischen Korrektur und Rücknahme erfasst wurde, stünde danach bei der
  falschen Mannschaft. Verworfen.

## Konsequenzen

- Protokolländerung → Relay muss neu gebaut werden (Auto-Deploy beim main-Merge).
- Eine alte App verwirft Syncs mit der neuen Art; der Schalter verhindert, dass ein neues
  Tablet sie an einen alten Host sendet.
- Im Doppel bleibt die Spielerposition in früheren Sätzen unscharf (nur innerhalb einer Mannschaft).
- Nach einer Tablet-Übernahme gehen beim Host die Ballwechsel-Details früherer Sätze verloren.
- Eine zurückgestufte App kann `zettel/*.json` mit der neuen Art nicht laden.
