# 0105 — Ansage bei Netzausfall: harte Frist und Merker im Kern, Ersatzstimme an einer Stelle, nie mischen

- **Status:** accepted
- **Datum:** 2026-11-16

## Kontext

Fällt in der Halle das Internet aus, hängt die Ansage. Befund am Code (Stand v0.9.437):

- Der HTTP-Client entsteht je Anfrage neu und kennt nur eine Gesamtfrist (10 s, Gemini 20 s),
  keine Verbindungsfrist.
- Scheitert „Ansage in Teilen", folgt ein **zweiter Versuch für die ganze Ansage** — im PC-Weg
  (`azure_tts_speak`) und im Seiten-Weg (`seiten_cloud_audio`). Bei totem Netz wartet die
  Ansage damit zweimal die volle Frist ab.
- Das Frontend setzt um die Vertonung keine eigene Frist; Fristen gibt es nur für die
  Wiedergabe.
- Niemand merkt sich einen Ausfall. Jede folgende Ansage wartet wieder genauso lange.
- Wer bei einem Fehler spricht, entscheidet am PC das Frontend (`playPrepared` → `sprich`),
  für die Ansage-Seiten der Kern (`ansage_vertonen`). Eine neue Ersatzstufe müsste an zwei
  Stellen eingebaut werden.

Die Spec [ansage-offline-stimme](../features/ansage-offline-stimme.md) verlangt: höchstens 5 s
Stille bei der ersten Ansage nach einem Ausfall, danach kein Warten mehr, und eine
Offline-Stimme (ADR 0104) als erste Ersatzstufe.

## Entscheidung

1. **Harte Frist im Kern.** Die ganze Vertonung einer Ansage läuft unter einer Frist
   (`tokio::time::timeout`), im PC-Weg und im Seiten-Weg. Dazu ein geteilter HTTP-Client mit
   Verbindungsfrist. Die Frist deckt auch, was HTTP-Fristen nicht decken (hängende
   Namensauflösung, stumme Gegenstelle).
2. **Fehlerklasse an der Quelle.** Die vier Dienst-Module melden `Netz`, `Drossel` (429),
   `Zugang` (401/403) oder `Inhalt`. Nur `Netz` (Verbindungsfehler, Zeitüberschreitung, harte
   Frist, 5xx) gilt als Ausfall.
3. **Kein zweiter Versuch nach `Netz`, `Drossel`, `Zugang`.** Den Versuch „als Ganzes" gibt es
   nur noch nach `Inhalt`.
4. **Merker je Sprachdienst in `AppState`.** Ein reiner Zustandsautomat mit gereichter Uhr.
   Solange er gesetzt ist, geht keine Vertonung ins Netz; eine nicht vollständig gecachte
   Ansage geht sofort an die Ersatzstimme. Der Merker lebt bewusst **nicht** in der
   `SyncEngine` — die wird bei jedem Speichern der Einstellungen neu gestartet.
5. **Hintergrundprüfung ohne Kosten.** Nur solange der Merker gesetzt ist, baut der Kern im
   Takt eine TLS-Verbindung zum Host des Dienstes auf. Gelingt sie, fällt der Merker. Ohne
   Erfolg läuft er nach einer Höchstdauer ab.
6. **Gürtel im Frontend.** `playPrepared` wartet auf den Kern höchstens Frist + 0,5 s.
7. **Die Ersatzstimme wählt der Kern, an einer Stelle.** Eine Funktion liefert den Ton der
   Ersatzstimme: Offline-Stimme (wenn eingeschaltet, bereit, deutsche Ansage), sonst
   Windows-Stimme. PC-Weg und Seiten-Weg benutzen dieselbe Funktion. Die Reihenfolge „Cloud →
   Ersatz → Browser" bleibt im Frontend, wie sie ist; der Abspielbericht nennt die Stufe.
8. **Nie mischen.** Fehlt bei einem Ausfall auch nur ein Teil, spricht die Ersatzstimme die
   ganze Ansage. Eine vollständig gecachte Ansage wird dagegen immer aus dem Cache gespielt,
   auch bei gesetztem Merker.
9. **„Lokal (ohne Internet)" ist ein Wert von `AzureTtsConfig.dienst`** (`"lokal"`), kein
   eigener Schalter. Weil ein unbekannter Dienst heute als Azure gilt, lernt schon die
   Version, die die Offline-Stimme einführt, den Wert als „kein Cloud-Dienst" zu lesen; die
   Version, die ihn wählbar macht, setzt die Rückweg-Grenze (ADR 0101) auf jene Version.

Ausgangswerte, vorbehaltlich der Messung: Frist 2,5 s, Verbindungsfrist 1,5 s, Prüftakt 15 s,
Höchstdauer des Merkers 10 Minuten.

## Alternativen

- **Nur kürzere HTTP-Fristen.** Verworfen: Eine hängende Namensauflösung und eine stumme
  Gegenstelle liegen außerhalb der HTTP-Frist; ohne Merker wartet jede Ansage erneut.
- **Netzstatus von Windows abfragen.** Verworfen: Ein Hotspot mit Anmeldeseite und ein Router
  ohne Internet melden „verbunden".
- **Probe-Vertonung als Hintergrundprüfung** (`tts_dienste_pruefen`). Verworfen: bucht Zeichen
  je Probe.
- **Merker in der `SyncEngine`.** Verworfen: geht bei jedem Speichern verloren.
- **Neue Ersatzstufe im Frontend (`sprich`).** Verworfen: Der Seiten-Weg bräuchte eine zweite
  Umsetzung, die Reihenfolge liefe auseinander.
- **Ganze Kette in den Kern ziehen.** Verworfen: Der Kern müsste die MP3-Antworten der Dienste
  selbst dekodieren; großer Umbau ohne Gewinn für dieses Ziel.
- **Fehlende Teile von der Ersatzstimme sprechen lassen (mischen).** Verworfen (Entscheidung
  des Nutzers): zwei Stimmen in einem Satz klingen schlechter als eine Ersatzstimme.
- **Eigener Schalter „offline" neben der Dienstwahl.** Verworfen: Zwei Felder können sich
  widersprechen; die Dienstwahl ist schon die Stelle für „wer spricht".

## Konsequenzen

- Ein langsames, aber lebendes Netz kann die Ersatzstimme auslösen. Die Frist ist eine
  Konstante und wird erst nach der Messung festgelegt.
- Eine Ansage, die heute nach einem Teil-Fehler als Ganzes doch noch gelang, geht bei
  Netzfehlern künftig direkt an die Ersatzstimme.
- Die Hintergrundprüfung kann „erreichbar" melden, obwohl die Vertonung scheitert. Dann
  kostet die nächste Ansage einmal die Frist, und der Merker steht wieder.
- Bei einem langen Ausfall wartet nach jedem Ablauf der Höchstdauer eine Ansage einmal die
  Frist ab. Das ist der Preis dafür, dass ein Fehler in der Prüfung die Cloud-Stimme nicht
  dauerhaft abschaltet.
- Ansagen mit Tabletbedienung oder Schiedsrichtern sind bei einem Ausfall fast nie
  vollständig im Cache — dort trägt die Ersatzstimme, nicht der Vorab-Bestand (ADR 0106).
- Kein neues gespeichertes Feld für die Ausfall-Erkennung; der Rückweg bleibt frei. Der Wert
  `lokal` verschiebt die Rückweg-Grenze einmal.
- Der Rückfall-Banner nennt künftig die Stufe, die spricht.

## Ergebnis der Messung

Offen — wird nach E0 hier eingetragen (Zeit bis zum Fehler je Ausfallart, Wirkung der
Verbindungsfrist, festgelegte Konstanten).
