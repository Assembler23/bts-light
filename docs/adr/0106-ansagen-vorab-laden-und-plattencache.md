# 0106 — Ansagen vorab laden: das Frontend baut die Teile, der Kern lädt sie auf die Platte; Verfall nach 60 Tagen

- **Status:** accepted
- **Datum:** 2026-11-16

## Kontext

Liegen alle Teile einer Ansage im Cache, braucht sie kein Netz („Ansage in Teilen",
v0.9.366). Die Spec [ansage-offline-stimme](../features/ansage-offline-stimme.md) will das
gezielt nutzen: Teile kommender Ansagen laden, solange Netz da ist — laufend für die nächsten
Spiele und auf Knopfdruck für das ganze Turnier.

Befund am Code (Stand v0.9.437):

- Den Ansagetext baut **das Frontend** (`prepare*` in `io/announcer.ts`), samt Wörterbuch,
  Stimme je Disziplin und Tempo. Der Kern schneidet ihn an den Pausen und cacht je Teil.
- Der Cache-Schlüssel entsteht in vier Dienst-Modulen je für sich.
- Der PC-Weg cacht auf der Platte (`tts-cache/`), **ohne Verfall** — ein offener
  Roadmap-Befund. Die Ansage-Seiten cachen nur im Arbeitsspeicher (Kriterium 10 der Spec
  `ansage-im-browser`: „nie auf die Platte").
- Der Cache schreibt nicht atomar; gelesen wird alles, was nicht leer ist.
- Je Ansage gehen bis zu drei Anfragen gleichzeitig hinaus; eine Grenze über mehrere Ansagen
  gibt es nicht.

## Entscheidung

1. **Das Frontend baut die Vorab-Teile mit denselben Funktionen wie die echte Ansage** und
   gibt sie dem Kern zum Vertonen ohne Wiedergabe. Ein Pflicht-Test hält fest, dass ein
   Vorab-Teil denselben Schlüssel bekommt wie derselbe Teil der echten Ansage.
2. **Ein geteilter Cache-Schlüssel im Kern**, den Vertonung und Abfrage benutzen. Die Frage
   „ist diese Ansage bereit?" beantwortet der Kern am Cache selbst.
3. **Der Cache ist der Fortschritt.** Kein eigener Speicher für „schon geladen"; ein
   abgebrochener Lauf setzt von selbst fort, ein Wechsel von Stimme, Tempo oder Wörterbuch
   macht alte Teile von selbst wirkungslos.
4. **Ein Plattencache für alle Wege.** Auch Ansage-Seiten und TV-Ansage-Modus lesen und
   schreiben `tts-cache/`. Kriterium 10 der Spec `ansage-im-browser` wird damit bewusst
   aufgehoben (Entscheidung des Nutzers).
5. **Verfall über das Änderungsdatum.** Es dient als „zuletzt benutzt" und wird bei einem
   Treffer höchstens einmal täglich erneuert. Was 60 Tage unbenutzt ist, wird beim
   Programmstart und einmal täglich gelöscht. Dazu ein Knopf „Ansage-Speicher leeren". Der
   Cache ist nicht ans Turnier gebunden — Namen kehren beim nächsten Turnier wieder.
6. **Atomar schreiben, beim Lesen prüfen.** Nebendatei, dann umbenennen; ein Eintrag, der
   sich nicht als Ton lesen lässt, gilt als nicht vorhanden und wird gelöscht.
7. **Die echte Ansage geht vor.** Vorab laden schickt eine Anfrage zur Zeit und wartet,
   solange eine echte Ansage vertont wird. Über alle Wege gehen höchstens drei Anfragen
   gleichzeitig an einen Dienst.
8. **Drossel des Anbieters:** Bei 429 wartet nur das Vorab laden (nach `Retry-After`, sonst
   wachsend). Trifft eine 429 eine echte Ansage, bricht der Vorab-Lauf ab.
9. **„Vorab laden" schaltet „Ansage in Teilen" zwingend ein.** Ganze Ansagen vorab zu laden
   wäre wirkungslos, weil das Feld erst bei der Vergabe feststeht.
10. **Umfang:** laufend die nächsten 20 Spiele mit feststehender Paarung plus Grundstock; der
    Knopf alle angesetzten Spiele mit bekannten Spielern, je Seite in beiden Formen. Keine
    Kandidaten offener Paarungen. Nur am Master; Slaves laden nicht vorab.
11. **Kosten sichtbar:** eigene Spalte „vorab" im Zeichenzähler; der Knopf zeigt vorher
    Anbieter, Anfragen und Zeichen und verlangt eine Bestätigung.

## Alternativen

- **Der Kern baut den Ansagetext selbst.** Verworfen: eine zweite Umsetzung von Wörterbuch,
  Sprachwahl und Satzbau — jede Abweichung macht das Vorab laden wirkungslos und kostet
  Zeichen doppelt.
- **Schlüssel im Frontend nachbauen.** Verworfen: vier Stellen, die auseinanderlaufen können.
- **Liste „geladen" im Arbeitsspeicher oder als Datei.** Verworfen: veraltet nach jeder
  Änderung von Stimme, Tempo, Dienst oder Wörterbuch; als Datei ein neues gespeichertes Format
  (ADR 0101) mit eigener Konsistenzfrage.
- **Verfall über das Zugriffsdatum.** Verworfen: auf NTFS unzuverlässig.
- **Cache beim Turnierwechsel leeren.** Verworfen (Entscheidung des Nutzers): Dieselben
  Spieler kommen wieder; der Bestand spart Zeichen und Wartezeit.
- **Seiten weiter nur im Arbeitsspeicher.** Verworfen: Die Seiten hätten bei einem Ausfall
  nichts vom Vorab-Bestand und zahlten jeden Teil doppelt.
- **Vorab laden parallel zur echten Ansage.** Verworfen: Beide teilen sich den Schlüssel und
  damit die Drossel des Anbieters.

## Konsequenzen

- Namen gehen **früher und breiter** an den gewählten Anbieter: vor dem Aufruf, beim Knopf
  auch für Spiele, die nie gerufen werden. Deshalb Standard aus und Bestätigung; CLAUDE.md
  bekommt dazu einen Absatz im Abschnitt Datenschutz.
- Ton mit Namen liegt bis zu 60 Tage nach der letzten Benutzung auf der Platte, jetzt auch
  der für Ansage-Seiten. Dafür hat der Cache erstmals einen Verfall; der Roadmap-Befund dazu
  ist erledigt.
- Wegen „nie mischen" (ADR 0105) hilft der Bestand bei einem Ausfall nur vollständig
  geladenen Ansagen. Tabletbedienung und Schiedsrichter sind selten vorab bekannt.
- Vorab laden verbraucht Zeichen. Der Bestand des Knopfes wird nach einer Änderung von
  Stimme oder Tempo nicht von selbst neu geladen.
- Der erste Programmstart nach dem Update räumt einen womöglich großen alten Cache — im
  Hintergrund.
- Die Vorversion liest den Cache unverändert; der Rückweg bleibt frei.
