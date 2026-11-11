# 0085 — Spielende: „überholt", geparktes Ergebnis und Nachzügler-Fenster

- **Status:** accepted
- **Datum:** 2026-11-11
- Ergänzt [ADR 0018](0018-ergebnis-weg-verlustsicher.md) (Ergebnis-Weg),
  [ADR 0014](0014-punktverlauf-expliziter-rally-frame.md) (Punktverlauf) und
  [ADR 0038](0038-ereignisse-append-only.md) (Zettel-Ereignisse).
- Spec: [`tablet-stand-robust`](../features/tablet-stand-robust.md).

## Kontext

Ein Ergebnis-Auftrag, dessen Spiel nicht mehr auf dem Feld liegt, wird endlos wiederholt
und sperrt „Ergebnis übermitteln" für das Folgespiel. Das `finalized`-Signal, das ihn lösen
sollte, fällt aus, wenn Hand-Ergebnis und neue Zuweisung im selben BTP-Poll liegen. Wurde
das Spiel dagegen ohne Ergebnis vom Feld genommen, ist der Auftrag die einzige digitale
Kopie. — Punktverlauf und Zettel-Ereignisse reißen nach einem Abriss Host↔Relay oder einem
Host-Neustart ab; ein Voll-Sync nach dem Ergebnis wird am Match-Filter verworfen.

## Entscheidung

1. **Neue Host-Antwort „überholt"** (`superseded`, nie `ok`, nie `permanent`): nur in den
   bestehenden Absage-Zweigen von `process_result`, wenn das Spiel in BTP beendet ist und
   einen Sieger hat. Das Tablet lässt den Auftrag dann los.
2. **Sonst parken statt verwerfen:** Bei Spielwechsel oder geleertem Feld wandert der
   Auftrag in eine eigene, getrennt gespeicherte Liste; das Folgespiel ist nie blockiert.
   Das Tablet zeigt ein Band, prüft im Hintergrund weiter und fragt nach, wenn das Spiel
   aufs Feld zurückkommt. Verwerfen ist jedem Bediener nach einer zweiten Abfrage möglich.
3. **Voll-Sync an drei Stellen** (bei `match_assigned` fürs geladene Spiel, vor und nach
   dem Ergebnis) und ein **Nachzügler-Fenster von 120 s** am Host (und als Durchleitung am
   Relay) für Voll-Syncs des gerade beendeten Spiels — auch bei neu belegtem Feld.

## Verworfene Alternativen

- **Auftrag beim Spielwechsel still verwerfen:** vernichtet das Ergebnis, wenn das Spiel
  ohne Ergebnis vom Feld ging; `clearMatch` kann zudem aus einem kurzzeitig leeren Snapshot
  stammen.
- **`permanent` bei „Spiel hat schon einen Sieger":** kollidiert mit der Korrektur durch die
  Turnierleitung nach „Wieder öffnen".
- **Meldung in der Turnierleitungs-Oberfläche:** mehr Wire-Fläche; das Band am Tablet
  genügt vorerst.
- **Verwerfen nur mit Schiri-PIN / gar nicht:** Das Band bliebe ohne PIN-Inhaber hängen.
- **Verlauf und Ereignisse im Ergebnis mitschicken:** garantiert vollständig, aber Eingriff
  in das Ergebnis-Format und größerer Ergebnis-Versand.
- **Bestes Bemühen ohne Fenster:** WS-Sync und HTTP-Ergebnis haben keine Ordnung; eine
  Karte könnte auf dem Zettel fehlen.
- **Relay puffert in der Lücke:** hielte Sanktionsdaten im Relay (ADR 0037 schließt das
  aus).

## Folgen

- R5 bleibt: kein neuer Annahme-Pfad; „überholt" ist eine Absage mit Zusatzinformation.
- Gegen ein altes Relay kommt „überholt" nicht an; das Tablet parkt dann (sicherer
  Rückfall).
- Nach einem Host-Neustart innerhalb der 120 s ist der Merker weg und ein Nachzügler wird
  verworfen; der Sync vor dem Ergebnis deckt den Normalfall.
- Der Bediener sieht mit dem Band einen neuen Zustand; das Handbuch beschreibt ihn.
