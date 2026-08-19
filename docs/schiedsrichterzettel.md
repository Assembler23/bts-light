# Schiedsrichterzettel drucken

Spezifikation: [features/schiedsrichterzettel-druck.md](features/schiedsrichterzettel-druck.md) ·
ADR [0037](adr/0037-zettel-ereignisse-eigener-strom.md) ·
[0038](adr/0038-ereignisse-append-only.md) ·
[0039](adr/0039-zettel-html-im-webview.md)

Für jedes mit Tablet gezählte Spiel lässt sich ein ausgefüllter Spielzettel
drucken: Punktverlauf, Aufschlagfolge, Karten, Verletzungen, Unterbrechungen,
Überstimmungen und Zeiten.

> **Internes Turnier-Archiv — kein amtlicher Beleg.** Der Vermerk steht auf jedem
> Zettel. Kein Protestverfahren, keine Verbandsklärung; offizielle Turniere laufen
> weiter über das Original-BTS ([umpire-mode.md](umpire-mode.md)).

## Erfassen (am Zähltablett)

Voraussetzung ist der **Schiri-Modus**: ⚙ → PIN → „Schiri-Modus: an".
Dann erscheint in der Ansage-Leiste der Knopf **„Karte / Verwarnung"**, der
zuerst nach der Art des Vorgangs fragt:

| Vorgang | Spieler nötig |
|---|---|
| Karte / Verwarnung (gelb, rot, schwarz) | ja |
| Behandlung beginnt / endet | nein |
| Unterbrechung | nein |
| Überstimmung | nein |
| Oberschiedsrichter gerufen | nein |

Erfassbar ist all das **auch in den Pausen und vor dem ersten Aufschlag** — dort
passieren die meisten dieser Vorgänge. Das Fenster schließt sich von selbst,
sobald die Pause endet.

**Die rote Karte ist nur wählbar, wenn ihr Punkt auch zählen kann.** In einer
Pause, nach Spielende oder unmittelbar nach dem letzten Punkt (700-ms-Sperre
gegen Doppeleingaben) ist sie ausgegraut. Grund: Sonst würde der Punkt still
verschluckt, die Karte aber trotzdem protokolliert — auf dem Zettel stünde eine
rote Karte ohne den Punkt, den sie erzeugt.

### Korrigieren

Ein **Undo** („↩") nimmt nicht nur den Punkt zurück, sondern auch die Ereignisse,
die danach erfasst wurden.

> **Zurückgenommenes verschwindet nicht.** Es steht auf dem Zettel
> **durchgestrichen** in der Protokollzeile und fehlt nur im Raster. Für einen
> Archivbeleg ist das ehrlicher als spurloses Verschwinden — aber es heißt auch:
> **eine versehentlich vergebene Karte bleibt sichtbar.** Wer das nicht möchte,
> muss vor dem Bestätigen genau hinsehen.

### Gerätewechsel

Übernimmt mitten im Spiel ein anderes Tablet das Feld, gehen **keine Ereignisse
verloren**. Der Turnier-PC hält die Wahrheit; das neue Gerät kann nichts
wegnehmen, was es nicht kennt. Ein Tablet, das offline war, bringt seinen
Rückstand beim Wiederverbinden nach.

## Drucken

**Am Turnier-PC:** In der Feldübersicht am belegten Feld und in der Liste der
abgeschlossenen Spiele steht ein Drucker-Symbol. Über der Liste gibt es
zusätzlich **„Zettel drucken"** für den **Stapeldruck** einer ganzen Runde
(höchstens 40 Spiele je Auftrag).

**Auf der Turnierleitungs-Seite:** im Kebab-Menü der Feldkachel unter
**„🖨 Zettel"**.

Beide zeigen eine Vorschau; **Drucken** öffnet den Druckdialog. „Als PDF
speichern" läuft über den Systemdialog des Druckers — deshalb braucht bts-light
dafür weder eine PDF-Bibliothek noch eine zusätzliche Datei-Berechtigung.

Das Blatt ist **A4 quer**. Je Satz ein Rasterblock; ab Ballwechsel 61 läuft er in
einer zweiten Zeilengruppe weiter.

## Was es *nicht* gibt

- **Keinen Zettel für Spiele ohne Tablet.** Ein von Hand eingetragenes Ergebnis
  hat keine Datenbasis; ein halb ausgefüllter Bogen wäre irreführender als keiner.
  Der Knopf erscheint dort gar nicht erst.
- **Kein nachträgliches Bearbeiten** eines abgeschlossenen Zettels.
- **Keine Aufschlagfolge bei Altbestand.** Spiele, die vor dieser Version gezählt
  wurden, haben kein `serve_start`. Der Zettel fällt dann im Doppel auf zwei
  Zeilen (eine je Mannschaft) zurück und sagt das oben dazu.

## Datenschutz

Der Zettel trägt **Spielernamen** und, falls turnierweit zugeschaltet, den
**Verein** — das ist sein Zweck. Kein Geburtsjahr, keine Lizenznummer.

Karten sind personenbezogene Sanktionsdaten und erscheinen **ausschließlich auf
dem Zettel**: nie im Anzeige-Zustand der Turnierleitungs-Seite, nie im
Punktverlauf-Graph, nie im badhub-Push, nie im Liveticker. Ein Wächter-Test
erzwingt das (`sanktionsdaten_erreichen_den_anzeige_zustand_nie`) — es ist keine
Zusage, sondern eine Typ- und Testgrenze.

In den gespeicherten Dateien (`zettel/<slug>.json`) stehen **keine Namen**, nur
Mannschafts- und Spieler-Nummern. Die Namen kommen erst beim Drucken aus dem
BTP-Snapshot.

Der Cloud-Relay **hält keine Zettel vor**. Ist der Turnier-PC getrennt, gibt es
keinen Zettel — und kein zwischengespeichertes Dokument.
