# Ballzähler

Am Zähl-Tablet lassen sich die verbrauchten Federbälle eines Spiels zählen.
Die Zahl geht mit dem Ergebnis an den Turnier-PC und von dort als
`Match.Shuttles` nach BTP — niemand tippt sie mehr ab.

Spec: [features/tablet-ballzaehler.md](features/tablet-ballzaehler.md) ·
ADR [0093](adr/0093-ballzahl-reist-mit-dem-ergebnis.md) · BTP-Messung:
[btp_protocol.md](btp_protocol.md) („Ballzahl `Match.Shuttles`").

## Einschalten

Der Schalter heißt in der Konfiguration `shuttle_counter` und steht
standardmäßig auf **aus**. Ein Setup-Schalter in der App folgt mit dem
Turnierleitungs-Teil (PR 3 der Spec); bis dahin lässt er sich nur in der
`config.json` setzen (App vorher beenden).

Der Schalter erreicht das Tablet über `MatchBrief.shuttleCounter` — mit der
nächsten Spielzuweisung. Ein Tablet an einem älteren Turnier-PC zeigt keinen
Zähler.

## Am Tablet

In der Kopfzeile steht während des Spiels **− 🏸 Zahl +**.

- Start bei **0**; jeder Ball wird mit **+** getippt, auch der erste.
- **−** nimmt einen versehentlichen Tipp zurück. Bereich 0–99.
- „Punkt zurück" (↩) ändert die Ballzahl **nicht**.
- Jeder am Tablet darf zählen, ohne PIN.
- Die Zahl überlebt Neuladen, Pausen, die Seiten-Korrektur und den Wechsel
  auf ein anderes Tablet (sie reist im gespiegelten Stand mit). Ein
  Ersatz-Tablet ohne übernommenen Stand beginnt bei 0.
- Mit dem **Übermitteln** des Ergebnisses ist die Zahl eingefroren, bis das
  Ergebnis am Tablet wieder geöffnet wird. Auch ein gepuffertes oder
  geparktes Ergebnis trägt die Zahl vom Zeitpunkt des Übermittelns.
- An einem Spiel, das die Turnierleitung schon von Hand gewertet hat, ist der
  Zähler gesperrt.

Technik: reine Zähl-Logik in `src/io/ballzaehler.mjs` (Test
`scripts/test-ballzaehler.mjs`), Inline-Kopie in `tablet.html` (Drift in
`scripts/test-inline-kopien.mjs`). `STATE.shuttles` liegt im gespiegelten
Stand (`standFuerSpiegel`), aber **nicht** im Undo-Stand (`snapshot`). Die
Anzeige-Zustände für Monitore tragen die Zahl nie
(`relay_proto::schlanker_anzeige_stand`).

## Was der Turnier-PC damit macht

`process_result` prüft die Zahl (R5):

| Vom Tablet | Schalter | Nach BTP geht |
|---|---|---|
| 0–99 | an | die Zahl des Tablets |
| über 99 oder Unsinn | an | der Wert, den BTP schon hat — das Ergebnis wird trotzdem angenommen |
| fehlt (älteres Tablet) | an | der Wert, den BTP schon hat |
| beliebig | aus | der Wert, den BTP schon hat |

Kampflose Wertung, Walkover-Kaskade und Disqualifikation tragen nie eine
gezählte Zahl. Ein als „überholt" abgewiesenes Ergebnis schreibt nichts. Die
Nachschub-Queue sendet die Zahl unverändert nach.

Unabhängig vom Schalter trägt **jeder** Schreibvorgang an ein Spiel die
Ballzahl mit, die BTP hat — BTP liest ein fehlendes Feld als 0 und löschte
sonst eine von Hand eingetragene Zahl.

## Grenzen

- BTP sieht die Zahl erst mit dem Ergebnis, nicht während des Spiels.
- Sendet das Tablet dasselbe Ergebnis noch einmal, wird nicht erneut
  geschrieben — auch nicht bei anderer Ballzahl. Deshalb das Einfrieren.
- Trägt die Turnierleitung das Ergebnis ein, bevor das Tablet übermittelt,
  verfällt die Tablet-Zahl.
- Keine Anzeige auf Monitoren, Zähltafel, Stream, Schiedsrichterzettel oder im
  Liveticker. Liga-Spiele sind nicht abgedeckt.
