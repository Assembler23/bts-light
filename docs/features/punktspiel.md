# Punktspiel: Ligabegegnung aus badhub live zählen

> **Status (22.11.2026):** umgesetzt mit v0.9.463 — dritte Turnierquelle `punktspiel`: koppeln, lesen, Feldvergabe,
> Ergebnis. Entscheidung: [ADR 0107](../adr/0107-punktspiel-als-dritte-turnierquelle.md), in badhub ADR-0092
> („Live-Scoring für Punktspiele“) und dort `docs/features/live_scoring.md` (Vertrag der API).
> Entstanden aus dem Auftrag von Tilo vom 22.11.2026, ohne den `/idee`-Ablauf.

## Ziel

Ein Verein zählt ein Punktspiel (Ligabegegnung, meist acht Einzelspiele auf ein bis vier Feldern) mit denselben
Tablets, Monitoren und demselben Liveticker wie ein Turnier. Die Begegnung, die Felderzahl und die Aufstellung
stehen in badhub (Mein Badhub → Live-Scoring), bts-light ist das Zählwerk.

## Harte Bedingung: BTP- und badhub-Turniere laufen unverändert

Die Quelle `punktspiel` wird nur über die Weichen in `quelle/mod.rs` erreicht. Kein Aufruf an BTP, kein
Turnierkern, keine Lease, kein Ereignisprotokoll.

## Ablauf

1. In badhub das Heimspiel einrichten (Felder, Aufstellung) und „Kopplungscode erzeugen“.
2. In bts-light unter **Turnierquelle** „Punktspiel“ wählen, Code und PC-Namen eintragen, **Koppeln**.
3. Übertragung starten. Die Spiele erscheinen wie Turnierspiele: aufrufen, auf dem Tablet zählen, Ergebnis.
4. Feldvergabe und Ergebnis stehen sofort in badhub. Der Liveticker läuft unter der Kennung der Begegnung.

## Umsetzung

Alles in `src-tauri/src/quelle/punktspiel.rs`.

| Teil | Inhalt |
|---|---|
| Kopplung | `POST {basis}/koppeln` mit Code, Installations-ID und Name. Das Gerätetoken liegt verschlüsselt unter `punktspiele/<uuid>/geraet.bin` (Schlüssel `punktspiele/schluessel.bin`, Verfahren wie bei badhub-Turnieren). In `config.json` steht nur `punktspiel` (Kennung, Name, API-Basis, Gerätename), nie das Token |
| API-Basis | aus der Liveticker-Adresse: `…/api/live_update.php` → `…/api/ls/v1`. Folgt dem Testsystem-Schalter, nur https |
| Lesen | `stand()` fragt im Takt der Sync-Schleife (5 s) `GET {basis}/begegnung` mit `If-None-Match`. Das Paket wird nicht gespeichert |
| Abbildung | `abbilden()`: Match-ID = Spielnummer, Feld-ID = Feldnummer, je Spiel eine Klasse mit dem Disziplinnamen („1.HD“), Runde = „Heim – Gast“, Verein = Mannschaftsname. Offene Plätze der Aufstellung heißen „N. N.“ |
| Feldvergabe | `feld_meldungen()`: Aufruf = Spiel läuft auf Feld n, Freigabe = Spiel wieder offen. Ein beendetes Spiel wird nicht wieder geöffnet |
| Ergebnis | `ergebnis_meldung()`: Sätze, Sieger (`heim`/`gast`), Ausgang (`normal`, `kampflos`, `aufgabe`) |
| Meldung | `POST {basis}/begegnung/spiel` trägt immer den ganzen Zustand eines Spiels. Die Antwort ist das neue Paket |

### Netz

Koppeln und Schreiben brauchen Internet. Fällt es während des Spieltags aus, gilt der zuletzt gelesene Stand weiter
und die Tablets zählen weiter. Ein Ergebnis, das badhub nicht erreicht, lehnt bts-light dem Tablet gegenüber ab, das
Tablet behält es und schickt es erneut. Ein Nachschub wie bei BTP gibt es nicht.

Entzieht badhub den Zugang (Kopplung getrennt, neues Gerät gekoppelt, Recht entzogen) oder führt nuLiga die
Begegnung als gespielt, meldet die Übertragung das als Fehler, statt mit altem Stand weiterzulaufen.

### Was es nicht gibt

- Schiedsrichter-Besetzung und Zeilenfarbe werden nicht an badhub gemeldet (lokal erledigt).
- Check-In, TP-Datei, Zeitplan, Standby-PC und Ansage-Slave: nicht für Punktspiele.
- Die Aufstellung ändert man in badhub, nicht in bts-light. Änderungen erscheinen nach wenigen Sekunden.

## Tests

`src-tauri/src/quelle/punktspiel/tests.rs`: Paket aus badhub-JSON, Abbildung (Felder, Spieler, offene Plätze,
laufend, beendet), Zählweisen, Feldvergabe (Aufruf, Freigabe, Wechsel), Ergebnis, und der ganze Weg gegen eine
Attrappe der API (koppeln, lesen mit 304, melden, Ablehnung durch badhub, Netz weg, Zugang entzogen).

## Offen

- Öffentliche Anzeige in badhub an der Begegnung (badhub ADR-0092 Schritt 3).
- Zählen ohne bts-light (badhub ADR-0092 Schritt 4).
- Hallentest mit echten Tablets.
