# 0015 — Punktverlauf: eine JSON-Datei je Turnier beim Host

- **Status:** accepted
- **Datum:** 2026-08-11

## Kontext

Punktverläufe ([Spec](../features/punktverlauf-graph.md)) sollen dauerhaft
aufbewahrt und später von badhub konsumierbar sein. Der Host hat keine
verpflichtende Turnier-Kennung (turnier.de-GUID nur bei aktivem
Check-In); das bestehende `live-scores.json` ist flüchtig und
feld-zentriert.

## Entscheidung

Je Turnier **eine JSON-Datei** `punktverlauf/<slug>.json` im App-Datenordner:
Header (Turniername, Startdatum, GUID falls konfiguriert) +
`matches`-Map (match_id → Sätze als Punktfolgen, ohne Namen). Schlüssel
ist der Slug aus **Turniername + Startdatum** (Whitelist `[a-z0-9-]`,
Path-Traversal ausgeschlossen). Geschrieben wird nach dem
`persist_scores`-Muster: atomar (tmp + rename), Schreib-Lock,
best-effort, **debounced** (~3 s) plus sofort bei
Satzende/Resync/Finalisierung. Beim Start bzw. Turnierwechsel wird die
Datei des aktuellen Turniers geladen; alte Dateien bleiben liegen.

## Alternativen

- **`live-scores.json` erweitern** — verworfen: Datei wird je Feld
  geräumt/überschrieben, kein Turnier-Schnitt, badhub-Export unmöglich
  ohne Umbau.
- **Append-only-Event-Log (JSONL) mit Replay** — verworfen: Replay wird
  ein zweites Zustandsmodell, Resyncs duplizieren unbegrenzt
  (Kompaktierung nötig), badhub-Schnitt erfordert Aggregation; kein
  Vorbild im Repo. Die Absturzlücke des Debounce (~3 s) heilt ohnehin der
  Tablet-Resync (ADR 0014).

## Konsequenzen

- Die Datei ist zugleich das fertige Dokument für den späteren
  badhub-Push (Folge-Feature liest sie 1:1).
- Kein Personenbezug auf Platte → keine Lösch-/Art.-17-Pflichten an
  dieser Stelle; Namen holt die Anzeige zur Laufzeit.
- Namenskollision (gleicher Turniername + Datum) führt die Datei weiter —
  bewusst akzeptiert und dokumentiert.
