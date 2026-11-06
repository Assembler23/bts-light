# 0076 — Warteliste samt Kandidatennamen auf öffentlichen Anzeigen

- **Status:** accepted
- **Datum:** 2026-11-06

## Kontext

Die Warteliste der Turnierleitung (Reihenfolge der kommenden Spiele) und bei
offenen Paarungen die möglichen Teilnehmer („A oder B") waren bisher nur im
TL-Web hinter dem Gerätezugang sichtbar. Die Spielfolge-Anzeige
(Spec `spielfolge-anzeige`) hängt beides öffentlich in die Halle.

## Entscheidung

Freigegeben durch die Turnierleitung am 06.11.2026. Die Namen stehen ohnehin
auf Auslosungsbogen, Spielplan und Liveticker; der Zweck ist derselbe
(Spieler bereithalten, das richtige Spiel ans Feld bringen). Die Ausgabe
(`tablet/spielfolge.rs`) ist eine Positivliste: keine Lizenznummer, kein
Geburtsjahr, keine SR- oder Check-In-Daten, Nation/Verein nur nach den
Monitor-Schaltern, Setzplatz nie an Kandidaten einer offenen Paarung.

## Alternativen

Offene Paarungen nur als „aus Spiel 42" ohne Namen — verworfen, weil die
Spieler genau wissen wollen, ob sie gemeint sind.

## Konsequenzen

Wächter-Test `traegt_nie_lizenznummern_und_nation_nur_mit_schalter`. Jede
Erweiterung der Zeile muss die Positivliste bewusst ändern.
