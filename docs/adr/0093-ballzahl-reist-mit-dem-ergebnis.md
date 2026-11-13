# 0093 — Ballzahl: reist mit dem Ergebnis, lebt im Tablet-Stand, BTP-Wert gilt

- **Status:** accepted
- **Datum:** 2026-11-13

## Kontext

Am Zähl-Tablet sollen die verbrauchten Federbälle gezählt und nach BTP
(`Match.Shuttles`) übertragen werden (Spec
[tablet-ballzaehler](../features/tablet-ballzaehler.md)). Drei Fragen haben
mehrere tragfähige Antworten:

1. **Wann und wie wird nach BTP geschrieben?** BTP liest bei mehreren
   Match-Feldern (`CourtID`, `Official1ID`/`Official2ID`, `Highlight`) ein
   fehlendes Element als 0; ein nackter Match-Knoten nach dem Ergebnis kann das
   Ergebnis entwerten. Für `Shuttles` ist das Verhalten nicht gemessen.
2. **Wo lebt die Zahl während des Spiels**, und wie sieht die Turnierleitung
   sie?
3. **Wer gewinnt**, wenn Tablet, Turnierleitung und ein Handwert in BTP
   voneinander abweichen?

## Entscheidung

1. **Die Ballzahl reist ausschließlich im Ergebnis-`SENDUPDATE`.** Kein
   Schreiben je Ball, kein eigener Shuttles-Write. Eine nachträgliche Änderung
   geht nur über die vorhandene Ergebnis-Korrektur (vollständiger Rewrite mit
   ihren Sperren). Zeigt die Messung, dass BTP „fehlt" als 0 liest, reasserten
   **alle** Match-Writes den zuletzt gelesenen BTP-Wert — unabhängig vom
   Schalter.
2. **Die Zahl ist ein eigenes Feld im Tablet-Stand, außerhalb des
   Undo-Stapels.** Sie erreicht den Host über den vorhandenen Zustands-Spiegel
   (für die Anzeige am laufenden Spiel) und verbindlich über den Ergebnis-Body.
   Aus den öffentlichen Anzeige-Zuständen wird sie gestrichen.
3. **Verbindlich ist der Wert in BTP.** Die Tablet-Zahl gilt nur zusammen mit
   dem Tablet-Ergebnis, das BTP annimmt; ein überholtes Ergebnis nimmt seine
   Zahl mit ins Leere. Ab dem Übermitteln ist der Zähler eingefroren, damit die
   Idempotenz-Prüfung (`settled_ok`) nicht erweitert werden muss.
4. **Schalter und Fähigkeit in einem Feld** (`MatchBrief.shuttleCounter`): Das
   Tablet zählt nur, wenn der Host die Zahl auch schreibt.

## Alternativen

- **Laufend je Ball nach BTP schreiben.** Verworfen: zusätzlicher
  Schreibverkehr auf laufende Spiele unter dem Schreib-Gate, ungemessenes
  BTP-Verhalten, kein Nutzen für die Abrechnung.
- **Eigener Shuttles-Write nach Spielende.** Verworfen: Ein Match-Knoten ohne
  Ergebnis-Felder kann das Ergebnis in BTP entwerten; bräuchte eine eigene
  Messung und einen zweiten Schreibweg.
- **Zahl im Score-Frame (`score_update`).** Verworfen: Wire-Änderung an Tablet,
  Relay und Host nur für eine Anzeige; der Zustands-Spiegel trägt das Feld
  bereits.
- **Zahl als Zettel-Ereignis (`EventKind`).** Verworfen: Das Enum hat keinen
  Catch-All, ein älterer Host verwirft ganze Frames.
- **Zahl im Undo-Stapel.** Verworfen: „Punkt zurück" nähme stillschweigend
  einen Ball zurück.
- **Ballzahl in den Idempotenz-Vergleich aufnehmen.** Verworfen: Eine
  Abweichung bräuchte genau den eigenen Shuttles-Write, der oben ausscheidet.

## Konsequenzen

- Vor der Umsetzung steht ein Messwerkzeug am echten BTP; sein Ergebnis wählt
  zwischen „nur im Ergebnis-Write" und „überall reasserten".
- BTP zeigt die Zahl erst nach Spielende.
- Die Anzeige am laufenden Spiel ist nur so gut wie der Zustands-Spiegel (fehlt
  bei altem Relay oder Tablet ohne Verbindung).
- Trägt die Turnierleitung das Ergebnis vor dem Tablet ein, verfällt die
  Tablet-Zahl, sofern sie die vorbelegte Zahl nicht übernimmt.
- Nach Spielende ist die Zahl aus bts-light nur dort änderbar, wo die
  Ergebnis-Korrektur erlaubt ist; sonst direkt in BTP.
- Relay-Deploy vor dem Client, weil das Relay den `Result`-Frame Feld für Feld
  baut und `TlAction` typisiert durchreicht.
