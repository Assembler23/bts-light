# 0092 — Stream-Werbung: Markierung als eigene Datei, Flag im Abdruck, 503 statt leer

- **Status:** accepted
- **Datum:** 2026-11-12

## Kontext

Neben der Score-Leiste (ADR 0069) soll es eine zweite OBS-Ebene für Werbebilder geben
(Spec [stream-werbung](../features/stream-werbung.md)). Welche Bilder in den Stream dürfen,
legt ein Häkchen je Bild im Setup fest. Dafür sind fünf Fragen zu entscheiden:

- Wo liegt die Markierung?
- Wie kommt sie in die Cloud?
- Über welchen Weg holt die Seite ihre Daten?
- Wird die Ebene eine eigene Seite?
- Wie unterscheidet die Seite „leere Auswahl" von „Relay weiß noch nichts"?

Die letzte Frage ist heikel: Der Relay wird bei jedem Merge auf main automatisch neu
deployt und antwortet danach, bis der Host neu hochgeladen hat, mit 200 und leeren Listen.

## Entscheidung

1. **Eigene Datei `court-ad-stream.json`** (Menge von Dateinamen, Format wie
   `court-ad-bar.json`) im Werbe-Verzeichnis. Kein Feld in `AdStyle` und kein Umbau von
   `court-ad-bar.json`.
2. **`AdUpload.in_stream`** (`serde default`), positionsweise wie `inBar`, und das Flag ist
   **Pflichtbestandteil des Fingerabdrucks** `ad_abdruck_zeile`.
3. **Daten additiv in `/info/ad/state`**: `streamAds` (LAN Dateinamen, Cloud Indizes) und, nur
   in der Cloud, `streamVer` (Inhaltsmarke je Bild als Cache-Brecher). Kein neuer
   Daten-Endpunkt.
4. **Eigene Seite** `stream-werbung.html` unter `/stream/werbung` und `/{ns}/stream/werbung`,
   mit `__BASE__`-Ersetzung wie `stream.html`.
5. **Abriss-Semantik:** Der Stream fragt mit `?zweck=stream`. Darauf antwortet der Relay mit
   **503**, wenn die Anzeige-Lage gesperrt ist (ADR 0088), wenn kein Monitor-Upload vorliegt
   oder wenn kein Host da ist und die Bildliste leer ist. Das LAN antwortet mit 503, wenn
   Markierungen bestehen, die Bildliste aber leer ist. Die Seite wertet **nur** „200 mit
   Liste `streamAds`" aus; alles andere heißt „stehen lassen". Ohne `zweck` bleibt die Antwort
   wie heute.

## Alternativen

- **Feld `in_stream` in `AdStyle`/`court-ad-style.json`:** Es spart eine Datei, verändert aber
  `ist_vorgabe()`, die Filterlogik in `write_ad_style` und den Abdruck. Ein Rollback auf eine
  alte Version würde das Feld beim nächsten Schreiben verwerfen. Verworfen.
- **`court-ad-bar.json` zu einer Karte {Name → Ziele} umbauen:** Eine alte Version könnte die
  Datei nicht mehr lesen; ein Rollback zerstörte die Leisten-Markierungen. Verworfen.
- **Auswahl in der URL (`?ads=0,2`):** Die Cloud-Indizes rücken beim Löschen nach, und der
  Stream zeigte still falsche Sponsoren. Vom Nutzer zugunsten des Häkchens verworfen.
- **Eigener Daten-Endpunkt `/stream/werbung/state`:** sauber getrennt, aber eine zweite
  Projektion derselben Daten und eine zusätzliche Route in der Slave-Brücke. Verworfen;
  `?zweck=` ist schon Muster (`monitor_state`, ADR 0070).
- **Feld `known:false` im JSON statt 503:** Es bräuchte eine eigene Auswertung in jeder
  Seite, während 503 schon bedeutet „keine Daten, nicht leeren" (ADR 0088). Verworfen.
- **Parameter an `stream.html` statt eigener Seite:** `stream.html` hängt an Feldern, dem
  Feld-Zustand und dem WS-Nudge; die Werbe-Ebene braucht nichts davon. Verworfen.

## Konsequenzen

- Ein reines Umhaken kommt in der Cloud erst mit dem nächsten Abdruck-Takt (30 s) an.
- Das geänderte Abdruck-Format löst nach dem Update einmal einen Upload aus.
- Der Relay muss vor der App ausgeliefert werden; das ist durch den automatischen
  Relay-Deploy beim Merge gegeben.
- `streamVer` gibt es nur in der Cloud. Im LAN sind die Dateinamen eindeutig, und `ad_image`
  hat ein ETag.
- Ein Stream-Bild jenseits der Upload-Grenzen (24 Bilder bzw. 12 MB) fehlt nur in der Cloud.
- Über die Slave-Brücke gibt es keine Stream-Werbung. Ferne Hallen nehmen die Cloud-Adresse.
