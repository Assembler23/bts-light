# 0062 — TP-Upload: Ereignis-Auslöser zieht den Takt vor, nur für Auslosung und Paarung, mit Drossel

- **Status:** accepted
- **Datum:** 2026-10-28

## Kontext

Der Takt-Upload (ADR 0061) prüft die TP-Datei nach der Uhr, Standard alle
15 Minuten. Eine Auslosung oder eine nach einem Vorspiel gefüllte Paarung
erreicht die Turnierseite deshalb erst nach bis zu 15 Minuten. Ein kürzerer
Takt würde das lindern, kostete aber bei jeder geänderten Datei einen Lauf
unter dem Schreib-Gate (ADR 0060) und einen Upload — auch für Änderungen,
die badhub gar nicht aus der Datei braucht.

Messung der badhub-Seite (`LiveVerschmelzung`, 28.10.2026): Die Turnierseite
eines `tp_live`-Turniers übernimmt **Ergebnisse, Zwischenstände und das
Feld** aus dem Liveticker-Stand, solange das Spiel darin steht. **Namen,
Paarungen und neue Spiele** kommen dagegen nur aus der Datei — die
Verschmelzung fügt keine Zeilen hinzu und fasst Namen nie an. Eine
Paarung bleibt also „Sieger HF1“, bis eine neue Datei kommt.

## Entscheidung

1. **Auslöser = Kern je Spiel.** Der Sync-Lauf bildet nach dem
   Leer-Snapshot-Guard je Abruf einen Kern `Match-ID → (Draw, Entry1,
   Entry2)` (`badhub/tp_ereignis.rs`) und vergleicht ihn mit dem vorigen.
   Neue oder entfernte Match-IDs oder eine geänderte Paarung sind ein
   Ereignis. Ergebnisse, Feld, Aufruf, Schiedsrichter und Farben **nicht** —
   die bringt der Liveticker. **Kein** Ereignis sind außerdem: der erste
   Abruf nach dem Start (auch nach dem Sync-Neustart beim Speichern), ein
   leerer Stand nach gefülltem (BTP-Neustart — der alte Kern bleibt, das
   Wiederauffüllen zählt nicht) und ein Stand ohne gemeinsame Match-ID
   (anderes Turnier geöffnet; das fängt der GUID-Abgleich).
2. **Vorziehen, nicht senden.** Ein Ereignis zieht nur den nächsten Termin
   des Takts vor (`TaktZustand::ereignis`). Der Takt-Schritt selbst bleibt
   unverändert: Schnell-Hash, nur bei geänderter Datei Gate und Upload,
   dieselbe Anhalte-Regel. Im Einzelnen:
   - **Entprellung:** 30 s nach dem **letzten** Ereignis, gedeckelt auf
     2 min nach dem ersten und den ursprünglichen Termin.
   - **Untergrenzen:** `letzter Upload + Mindestabstand` (Takt und Knopf)
     und jede Bremse aus 429/503 `Retry-After`, Pause oder Nachversuch.
   - **Nicht verlieren:** Kommt ein Ereignis, während der Schritt fällig
     ist oder läuft, wird es nach dem Schritt angewendet.
   - **Einmal nachprüfen:** Ist die Datei bei der vorgezogenen Prüfung noch
     unverändert (BTP schreibt die Datei nach dem Protokoll), folgt genau
     ein weiterer Blick nach 90 s, dann der Takt.
3. **Abschaltbar und drosselbar.** `TpUploadConfig.ereignis_enabled`
   (Standard an, wirkt nur mit eingeschalteter Automatik) und
   `ereignis_minuten` (Mindestabstand, Standard 5, geklemmt auf ≥ 5). Beide
   werden je Abruf aus der geteilten Config gelesen — Abschalten im Setup
   wirkt ohne Neustart. Ohne Auslöser bleibt der reine Takt.
4. **Bremse beim Server bleibt.** badhubs 429/503 mit `Retry-After`
   setzen den Termin weiterhin hinaus (ADR 0061); badhub kann also auch
   zentral bremsen, ohne App-Update.

## Alternativen

- **Ergebnisse als Auslöser:** verworfen — die Turnierseite zeigt sie aus
  dem Liveticker; jedes Ergebnis kostete sonst einen Gate-Lauf.
- **Sofort senden statt vorziehen:** verworfen — zweiter Sendeweg neben dem
  Takt, eigene Fehlerbehandlung; das Vorziehen nutzt den geprüften Schritt.
- **Kürzerer Takt:** verworfen — kostet Lesen und Hashen auch ohne
  Änderung und trifft Auslosungen trotzdem nur im Raster.

## Folgen

- Eine Auslosung oder gefüllte Paarung steht typisch nach 30 s bis 1 min
  auf badhub (Entprellung + Prüftakt 5 s + Upload + Worker), höchstens
  einmal je Mindestabstand.
- Uploads je Stunde ≤ 60 / Mindestabstand + 60 / Takt; mit Standards 12 + 4.
- Ändert BTP etwas, das nur in der Datei steht, aber nicht im Kern
  (Verein, Setzplatz, Nachmeldung ohne neues Spiel), greift weiterhin der
  Takt.
