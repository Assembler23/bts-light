# 0026 — Spielliste: eine globale Reihenfolge, eine Liste, Status an der Zeile

- **Status:** accepted
- **Datum:** 2026-08-15
- **Ersetzt:** [ADR 0023](0023-manuelle-spielreihenfolge-praefix-je-halle.md)
  (Präfix je Halle) in der Hallen-Frage; die dort getroffenen Entscheidungen
  zu **atomaren Zügen** und zum **verpflichtenden gemeinsamen Sortier-Helfer**
  bleiben unverändert gültig.

## Kontext

ADR 0023 führte die manuelle Spielreihenfolge als **Präfix je Halle** ein.
Begründung damals: Ein Zug soll nicht über eine Hallengrenze hinweg wirken,
und die Warteliste war nach Hallen gruppiert — die Gruppe war zugleich die
Grenze der Ziehgeste.

Im Betrieb erwies sich beides als unpraktisch (Rückmeldung 15.08.2026):

- Die **Hallen-Gruppierung** in der Warteliste zerschneidet die Liste in
  Blöcke, obwohl die Turnierleitung die Spiele als **eine** Abfolge denkt.
- Die **vier Wartelisten-Abschnitte** („In Vorbereitung gerufen",
  „Spielbereit", „Noch nicht bereit", „Ohne Hallenzuordnung") zerlegen
  dieselbe Menge ein zweites Mal. Zusammen mit der Hallen-Gruppierung
  entstanden bis zu acht Blöcke für eine einzige Frage: „was kommt als
  Nächstes?"

Die zugrundeliegende Annahme von ADR 0023 — dass die Hallen-Trennung der
Sortierung eine **Schutzfunktion** hat — trägt zudem nicht: Ob ein Spiel auf
einem Feld der falschen Halle landen kann, entscheidet allein
`sync.rs::auto_assign` über `require_call` (`multi_hall && active_loc.is_none()`
⇒ nur in **diese** Halle gerufene Spiele) und über
`config.hall_allows_match`. Der Sortier-Präfix hat darauf keinen Einfluss —
ein in Halle A vorgezogenes Spiel bekommt schon heute global Rang 0, weil
`resolve_and_sort_key` den Rang in der Halle **des Matches** nachschlägt,
nicht in der des zu füllenden Felds.

## Entscheidung

1. **Eine globale Reihenfolge.** Der manuelle Präfix ist eine einzige
   geordnete `Vec<i64>` statt einer Map `Halle → Vec<i64>`.
   `QueueOrderStore::rank`/`reorder`/`retain` verlieren ihren
   Hallen-Parameter; `assign::ready_queue_for_hall` wird zu
   `assign::ready_queue` ohne Hallenfilter. Der Wire-Vertrag
   (`TlAction::QueueReorder`/`QueueOrderReset`) bleibt **unverändert** — er
   trug nie eine Halle.

2. **Eine Liste statt vier Abschnitten.** Die Warteliste ist genau ein
   Panel „Spiele" (alle angesetzten Spiele) neben „Beendete Spiele". Die
   bisherigen vier Untergruppen werden zu einem **visuellen Status an der
   Zeile** (gerufen · spielbereit · nicht bereit · ohne Hallenzuordnung).
   Sortierung: **gerufene Spiele oben angepinnt**, alles Übrige nach
   manueller bzw. BTP-Reihenfolge — die Spielbereitschaft ist damit kein
   Sortierkriterium mehr, sondern nur noch Anzeige.

3. **Keine Hallen-Gruppierung in der Spielliste.** Stattdessen trägt jede
   Zeile das **Hallen-Kürzel** (kürzestes eindeutiges Präfix über
   `state.halls`, Logik aus Commit `68c3052` wiederbelebt). Die Gruppierung
   der **Feldkacheln** nach Halle bleibt unangetastet — dort ist sie
   Voraussetzung des Feld-Rasters und bildet die physische Realität ab.

4. **Deckel global.** `QUEUE_LIMIT_PER_HALL` wird zu einem globalen
   `QUEUE_LIMIT`. Der bestehende Schutz aus der Review vom 14.08.2026 (ein
   Zug „ans Ende" darf nicht mehr Spiele in den Präfix ziehen, als die
   Oberfläche zeigen konnte) gilt damit über die Gesamtliste.

## Alternativen

- **Hallen-Präfix behalten, nur die Gruppierung entfernen**: verworfen — die
  Reihenfolge wäre dann nach Hallen getrennt, die Anzeige aber nicht. Zwei
  Spiele verschiedener Hallen ständen unmittelbar untereinander, ließen sich
  aber nicht gegeneinander ziehen. Genau die Inkonsistenz, die ADR 0023
  strukturell vermeiden wollte.
- **Spielbereitschaft als Sortierkriterium behalten** (gerufen → bereit →
  nicht bereit, nur ohne Überschriften): verworfen auf ausdrücklichen
  Wunsch — die Liste soll der geplanten Abfolge folgen, nicht dem
  Momentanzustand. Ein Spiel, das gleich spielbereit wird, soll nicht
  springen.
- **Nicht spielbereite Spiele ausblenden/kürzen**: verworfen — „alle
  angesetzten Spiele" ist die ausdrückliche Anforderung. Die Länge wird
  stattdessen durch schrittweises Nachladen beim Scrollen beherrscht.

## Konsequenzen

- Die „Hallenwechsel räumt den Präfix auf"-Semantik aus ADR 0023
  (`sync.rs::reconcile_queue_order` mit `keep_by_hall`) **entfällt
  ersatzlos** — bei einer globalen Liste hat ein Hallenwechsel keine
  Auswirkung mehr auf die Reihenfolge. Der zugehörige Test verschwindet
  mit ihr.
- Eine bestehende `queue-order.json` im alten Map-Format wird nicht
  migriert. Das Feld heißt neu `queue` statt `order`; die alte Datei parst
  dadurch weiterhin fehlerfrei (unbekanntes `order` wird ignoriert,
  `queue` fehlt ⇒ Default), das Turnier startet mit leerem Präfix und
  behält seine Turnierbindung. Kein Absturz, kein Datenverlust an anderer
  Stelle — verhaltensgleich zu „Datei fehlt".
- **Liveticker-Nebenwirkung:** `badhub/payload.rs::upcoming` schneidet nach
  15 Einträgen. Mit einer globalen Reihenfolge kann ein langer manueller
  Präfix aus einer Halle alle 15 Plätze belegen, sodass
  `display=next&halle=…` für die andere Halle leer bliebe. Das ist die
  direkte Folge davon, dass die Turnierleitung die Reihenfolge jetzt
  hallenübergreifend bestimmt — bewusst in Kauf genommen, aber in
  `docs/preparation.md` als Betriebshinweis festgehalten.
- Mischbetrieb ist unkritisch: Das Wire-Format ändert sich nicht, nur die
  Semantik. Ein älterer Browser-Client gegen einen neuen Host sortiert
  lediglich anders.
