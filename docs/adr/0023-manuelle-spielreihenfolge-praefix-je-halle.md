# 0023 — Manuelle Spielreihenfolge: Präfix je Halle, atomare Züge, ein Sortier-Helfer

- **Status:** accepted
- **Datum:** 2026-08-14

## Kontext

BTPs Sortierreihenfolge (`PlannedTime → DisplayOrder → MatchNr → ID`,
`assign::sort_key`/`sort_key_parts`) ist laut `docs/btp_protocol.md`
„die eine Definition, die überall gilt" und wird an **fünf** Stellen im
Code dupliziert genutzt (`sync.rs::auto_assign`, `tablet/tl.rs`,
`commands.rs`, `tablet/server.rs`, `badhub/payload.rs::upcoming`) — die
Doku warnt selbst ausdrücklich davor, dass eine unkoordinierte Änderung
an einer Stelle „jede Ansicht eine andere ‚nächste Begegnung'" zeigen
lässt. [Spielliste-manuelle-Reihenfolge](../features/spielliste-manuelle-reihenfolge.md)
führt eine manuelle, host-lokale Sortier-Überschreibung ein, die genau
dieses Risiko strukturell ausschließen muss statt es nur durch Disziplin
zu vermeiden. Vier der fünf Stellen kennen außerdem heute die
abgeleitete Halle eines Matches (`hall_for_match`) noch gar nicht — der
Präfix soll aber je Halle getrennt gelten.

Eine Messung (`btp_displayorder_probe.rs`, 14.08.2026, gegen TEST
Köpi-Cup) hat vorab geklärt: `Match.DisplayOrder` lässt sich per
`SENDUPDATE` **nicht** nach BTP zurückschreiben — stiller No-Op, exakt
wie beim bereits dokumentierten `LocationID`-Befund. Die manuelle
Reihenfolge kann also nur host-lokal geführt werden; ein
BTP-Rückschreib-Pfad entfällt als Alternative von vornherein.

## Entscheidung

Drei zusammengehörige Teilentscheidungen bilden ein Architekturmuster:

1. **Datenmodell:** Der manuelle Präfix ist je Halle eine geordnete
   `Vec<i64>` von Match-IDs, persistiert in einer eigenen
   turniergebundenen Datei (`queue-order.json`, Muster ADR 0022 /
   `AutoAssignExclusionStore`). Nur die tatsächlich manuell gezogenen
   Spiele stehen darin — alles danach folgt weiter unverändert BTPs
   eigener Reihenfolge, sie wird nicht mitgeführt.
2. **Zug-Übertragung:** Ein Drag wird als **atomare Einzel-Operation**
   `(match_id, before_match_id)` übertragen (Muster `OfficialReorder`),
   serverseitig auf die aktuell gültige Liste angewendet — kein
   Full-List-Overwrite, das eine zeitgleiche Änderung der jeweils
   anderen Oberfläche (TL-Web/Desktop) verschlucken könnte.
3. **Verpflichtender gemeinsamer Sortier-Helfer:** Alle fünf Call-Sites
   nutzen denselben `sort_key_with_manual_order`/`resolve_and_sort_key`
   aus `tablet/assign.rs`, abgesichert durch einen Cross-Site-
   Regressionstest, der die fünf produktiven Funktionen gegeneinander
   vergleicht. Das ist der eigentliche Architektur-Fußabdruck dieses
   ADRs — er verhindert das eingangs beschriebene Divergenz-Risiko
   strukturell statt durch Code-Review-Disziplin.

Gerufene Spiele (`preparation_call_ts` gesetzt) bleiben unabhängig vom
Präfix immer ganz oben — der neue Sortier-Schlüssel reiht `!called` als
erste Tupel-Komponente vor den Präfix-Rang ein, ohne die bestehende
Vorrang-Logik zu duplizieren.

## Alternativen

- **Rang-Zahl je Match (`HashMap<i64,i64>`)** statt geordneter Liste:
  verworfen — jede Einfügung „vor X" erzwingt Renormierung nachfolgender
  Ränge oder eine Lücken-Rang-Strategie mit eigener Reparatur-Logik bei
  Erschöpfung. Kein Mehrwert gegenüber dem simplen `remove`+`insert` der
  Vektor-Variante bei der erwarteten Präfix-Größe.
- **Vollständige Match-ID-Liste je Halle** (nicht nur Präfix, sondern
  die gesamte Reihenfolge): verworfen — der Brief verlangt explizit,
  dass alles hinter dem Präfix weiter BTPs Reihenfolge folgt; eine volle
  Liste müsste bei jeder BTP-Änderung (neues Match, verschobene
  `PlannedTime`) nachgeführt werden.
- **Voll-Overwrite statt atomarer Züge**: verworfen — verliert eine
  zeitgleiche Änderung der jeweils anderen Oberfläche.
- **Zentraler Hallen-Cache** (`hall_for_match`-Ergebnis einmal je
  Sync-Zyklus cachen) statt Auflösung pro Call-Site: verworfen für den
  ersten Wurf — die fünf Stellen laufen zu unterschiedlichen Zeitpunkten
  (Sync-Zyklus, Tauri-Command, HTTP-Request), ein veralteter Cache wäre
  ein neues Konsistenzrisiko. Ein zentrales `hall`-Feld direkt in
  `BtpMatch` zu schreiben würde außerdem R2 verletzen (BTP-Modell wird
  um einen host-lokalen, abgeleiteten Wert verunreinigt).
- **BTP-`DisplayOrder`-Rückschreiben**: durch Messung (14.08.2026)
  ausgeschlossen — BTP ignoriert den Write still, wie bei `LocationID`.

## Konsequenzen

- `badhub/payload.rs::build_tset`/`upcoming` erhalten eine neue Signatur
  (zusätzlicher Kontext-Parameter) — der Umbau mit dem größten
  Blast-Radius, da rund 15 bestehende Testaufrufe mechanisch angepasst
  werden müssen.
- Ein dritter turniergebundener Persistenz-Pfad neben `officials.rs` und
  `exclusion.rs`, gleiches `.tmp`+`rename`-Schreibmuster.
- Ältere App-Versionen ignorieren `queue-order.json` schlicht — Rollback
  bleibt gefahrlos; ohne die Datei startet jedes Turnier mit leerem
  Präfix (Verhalten identisch zu heute).
- Der Cross-Site-Regressionstest macht künftige Änderungen an einer der
  fünf Stellen strukturell auffällig, statt sich auf manuelle
  Aufmerksamkeit bei Code-Reviews zu verlassen.
