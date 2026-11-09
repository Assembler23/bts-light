# 0079 — Rotationsmodi: zwei Reihenfolgen in A, abgeleitete Vorrolle, eine Wahl-Funktion

- **Status:** proposed
- **Datum:** 2026-11-09

## Kontext

Die Officials-Rotation (ADR 0021/0022) kennt eine Reihenfolge. SR und AR werden nacheinander
als „erster Freier" gewählt. Nach Spielende rücken beide in der Folge `[SR, AR]` ans Ende, deshalb
bleiben die Dienste an den Personen kleben. Gewünscht sind drei Modi (Spec
[`sr-rotationsmodus`](../features/sr-rotationsmodus.md)): A getrennte Listen, B abwechselnd,
C Rollenwechsel je Person. Außerdem soll es eine Rolle je Person geben (beides / nur SR / nur AR).
Der Meetingpoint wählt seinen Vorab-SR (ADR 0072) heute über dieselbe Funktion `next_free`.

## Entscheidung

- **Modus A arbeitet mit zwei Reihenfolgen.** `order` bleibt erhalten (in B/C die gemeinsame
  Liste, in A die SR-Liste). Neu kommt `order_ar: Option<Vec<i64>>` dazu, angelegt erst beim
  ersten Schreibbedarf als Kopie von `order`. Es gibt keine Migration, und ältere Versionen
  können die Datei weiter lesen.
- **Die Vorrolle wird abgeleitet, nicht gespeichert.** Sie ergibt sich je Takt aus den beendeten
  Spielen, nach derselben Sicht wie die Einsatz-Ableitung (`effective`, ohne Walkover). Maßgeblich
  ist der Schlüssel `(finished_at oder 0, match_id)`.
- **Eine reine Wahl-Funktion je Feld** (`waehle_besetzung`) besetzt alle offenen Dienste eines
  Felds gemeinsam. C wählt personenweise mit Rückfall auf „erster Freier mit passender Rolle".
  Rotation, Meetingpoint-Vorab-SR, die geplante Zuteilung beim Aufruf (Spec `sr-ar-beim-aufruf`, ADR 0080) und die Vorschau nutzen diese Funktion. `next_free` als zweiter
  Wahlweg entfällt.
- **Die Vorschau „als Nächstes" rechnet der Host.** Er simuliert die Wahl-Funktion Runde für
  Runde, ohne etwas zu verändern, und liefert das Ergebnis an Desktop und TL-Web. Im Browser
  wird keine Regel nachgebaut.
- **Eine Rolle gegen den Dienst ist eine Warnung, kein Konflikt.** Sie läuft nicht über
  `pruefe_einteilung` und ist kein `ConflictKind`. Die Handeinteilung und der SR⇄AR-Tausch
  bleiben möglich, BTP gewinnt (R2).
- In **B und C** rücken nach Spielende `[AR, SR]` ans Ende, in **A** jede Person in ihrer
  eigenen Liste.

Ergänzt ADR 0021/0022 (Rotation, Turnierbindung), 0072 (Vorab-SR folgt Modus und Rolle) und
0078 (Belegung bleibt die einzige Quelle für „frei").

## Alternativen

- **Vorrolle speichern** (`OfficialExtra.letzter_dienst`): Das Lesen wäre billiger. Aber es
  entsteht eine zweite Wahrheit, die bei Hand- oder BTP-Änderungen nach Spielende, bei Walkover
  oder Neustart auseinanderläuft. Verworfen.
- **Vorrolle aus der Listenposition:** Das ist nur B mit `[AR, SR]` und keine echte Vorrolle.
  Verworfen.
- **Wahl je Dienst** (`next_free(dienst, …)`, zweimal aufgerufen): liegt näher am heutigen Code,
  macht C aber dienstweise. Wer zuletzt SR war, würde beim AR nicht bevorzugt. Verworfen.
- **Vorschau im Browser:** doppelte Logik in JavaScript, und die Vorrolle müsste in den Zustand.
  Verworfen.
- **`order` in `order_sr`/`order_ar` umbenennen:** sauberere Namen, aber mit Migration. Ältere
  Versionen sähen eine leere Liste. Verworfen.
- **Rolle nur in Modus A:** Dann wären „nur AR"-Personen in B/C ungeschützt. Verworfen auf Wunsch
  der Turnierleitung.

## Folgen

- Bestehende Turniere in B tauschen ab dem Update meistens die Paar-Rollen. Das steht im
  Changelog.
- Ein Downgrade verliert Rolle und AR-Liste beim nächsten Speichern, ohne Rückmeldung.
- Die zweite Liste und die Rollenpflege im TL-Web brauchen einen eigenen `TlAction` und einen
  Relay-Deploy vor dem Client-Release (PR 5).
