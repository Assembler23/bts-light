# 0014 — Punktverlauf: expliziter Rally-Frame vom Tablet

- **Status:** accepted
- **Datum:** 2026-08-11

## Kontext

Der Punktverlauf-Graph ([Spec](../features/punktverlauf-graph.md)) braucht
die Ballwechsel-Folge beim Host. Die erreicht ihn heute nicht: Tablets
senden Stand-Schnappschüsse (`ScoreUpdate`), nach Offline-Phasen nur den
Endstand; der volle Tablet-Zustand (`state_sync`) ist ein bewusst opaker
String für Geräte-Übernahmen.

## Entscheidung

Das Tablet überträgt den Verlauf **explizit**: je Ballwechsel ein
`TabletMsg::Rally`-Frame (match_id, Satz, laufende Nummer, Gewinner,
Stand), dazu `TabletMsg::RallySync` als **Komplett-Resync** nach Undo,
Satz-Wiedereröffnung, Reconnect, Reload und Geräte-Übernahme — der Sync
ersetzt den Host-Stand des Matches vollständig. Neue Felder tragen
`#[serde(default)]` (Muster `matchId`), Größen-Caps verteidigen den
Cloud-Weg.

## Alternativen

- **Host rechnet aus Schnappschüssen** — verworfen: Nach Reconnect/Undo
  entstünde ein erfundener oder lückenhafter Verlauf; der Graph würde
  lügen.
- **`state_sync`-String parsen** — verworfen: koppelt Host-Persistenz an
  das Tablet-interne Zustandsformat; jede Tablet-Änderung bräche die
  Verläufe stillschweigend.

## Konsequenzen

- Neues Wire-Vokabular in `relay-proto` und Durchleitung im Relay
  (Briefträger, nur Caps) — Rollout-Regel „Relay vor Client".
- Der Resync macht den Verlauf selbstheilend (Host-Neustart,
  Offline-Lücken) — genau die Fälle, die Schnappschüsse nicht abdecken.
- ~80 Bytes je Punkt zusätzlicher Tablet-Traffic (Größenordnung des
  ohnehin gesendeten `ScoreUpdate`), bewusst akzeptiert.
