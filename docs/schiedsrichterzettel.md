# Schiedsrichterzettel drucken

Spezifikation: [features/schiedsrichterzettel-druck.md](features/schiedsrichterzettel-druck.md) ·
[features/schiedsrichterzettel-autodruck.md](features/schiedsrichterzettel-autodruck.md) ·
ADR [0037](adr/0037-zettel-ereignisse-eigener-strom.md) ·
[0038](adr/0038-ereignisse-append-only.md) ·
[0039](adr/0039-zettel-html-im-webview.md) ·
[0042](adr/0042-stiller-druck-ueber-elementliste.md) ·
[0043](adr/0043-zettelblatt-nach-dbv-vorbild.md)

Für jedes mit Tablet gezählte Spiel lässt sich ein ausgefüllter Spielzettel
drucken: Punktverlauf, Aufschlagfolge, Karten, Verletzungen, Unterbrechungen,
Überstimmungen und Zeiten.

Offizielle Turniere laufen weiter über das Original-BTS
([umpire-mode.md](umpire-mode.md)).

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

> **Zurückgenommenes verschwindet nicht.** Es steht auf der Anhangseite des
> Zettels, ausdrücklich als „zurückgenommen" bezeichnet, und fehlt nur im Raster.
> Das ist ehrlicher als spurloses Verschwinden — aber es heißt auch: **eine
> versehentlich vergebene Karte bleibt sichtbar.** Wer das nicht möchte, muss vor
> dem Bestätigen genau hinsehen.

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

## Das Blatt

**A4 quer, nach dem Vorbild des DBV-Bogens** — der Bogen, den Schiedsrichter
kennen (ADR [0043](adr/0043-zettelblatt-nach-dbv-vorbild.md)):

- **Sechs Blöcke à 33 Spalten**, vier Zeilen je Block. Die erste Spalte trägt den
  Startstand, die übrigen 32 die Ballwechsel. Ein Satz beginnt immer in einem
  neuen Block und läuft, wenn er länger wird, im nächsten weiter. Reichen sechs
  Blöcke nicht, folgt eine zweite Seite mit verkürztem Kopf.
- **Schmale A/R-Spalte** vor dem Raster: „A" beim Aufschläger, „R" beim
  Rückschläger zu Satzbeginn. Ohne aufgezeichnete Aufschlagfolge bleibt sie leer —
  geraten wird nicht.
- **Im Einzel** stehen die Spieler in Zeile 1 und 3, die anderen beiden bleiben
  frei. Der Bogen hat immer vier Zeilen.
- **Kopf:** links Spiel-Nr., Disziplin, Feld und Datum · Mitte die beiden
  Mannschaftskästen mit den Marken „L" und „R" und dazwischen das Satzergebnis ·
  rechts Schiedsrichter, Aufschlagrichter, Beginn, Ende, Dauer. Oben links steht
  das **Turnierlogo** (sofern hinterlegt) und der Turniername — **kein
  Verbandslogo**, das ist geschützt.
- **Marker in der Zelle** in der gewohnten Konvention: **W** Warnung (gelb),
  **F** Fault (rot), **R** Oberschiedsrichter gerufen, **D** Disqualifikation.
- **Fuß:** Unterschriftszeilen „Schiedsrichter" und „Referee".
- **Vorkommnisse** (Karten, Behandlungen, Rücknahmen) stehen auf einer eigenen
  **Anhangseite** mit Uhrzeit, Satz, Stand und Art — der Bogen selbst hat dafür
  keinen Platz. Ein Spiel ohne Vorkommnisse hat auch keine Anhangseite.

Das Layout ist eine Elementliste in Millimetern
(ADR [0042](adr/0042-stiller-druck-ueber-elementliste.md)); Breite und Höhe des
Blatts sind Kompilierbedingungen, und Wächter-Tests prüfen zusätzlich am
erzeugten Blatt, dass sich nichts überdeckt und nichts über den Rand läuft.

**Musterblatt ansehen:** `cargo test --lib musterblatt -- --ignored --nocapture`
schreibt `target/musterblatt.html`.

## Vorabzettel (Leerzettel)

Für ein Spiel, das noch aussteht, gibt es den **Vorabzettel**: derselbe Bogen mit
gefülltem Kopf — Turnier, Disziplin, Spielnummer, Namen, Verein, dazu Feld, Halle
und Besetzung, **sofern sie schon feststehen** — und leerem Raster. Er wird von
Hand geführt.

Technisch ist das ein eigener Modus, **keine** Änderung des Archivabrufs: Ein
Spiel ohne Aufzeichnung liefert normal weiterhin nichts (404), vorab dagegen ein
vollständiges Blatt. An der Route hängt dafür `?vorab=1`.

**Wo er zu finden ist:**

- **Turnier-PC**, Tab „In Vorbereitung": Spiele anhaken, dann **🖨 Zettel**
  neben „Aufrufen" — ein Blatt je ausgewähltem Spiel, höchstens 40 je Auftrag.
- **TL-Web**, Spielliste: im ⋮-Menü jeder Zeile **🖨 Zettel (leer)**. Gedruckt
  wird über das Gerät, auf dem TL-Web läuft.

## Automatisch drucken

Ist im Setup **„Zettel bei der Feldvergabe automatisch drucken"** eingeschaltet,
geht das Blatt von selbst an den eingestellten Drucker, sobald ein Spiel

- **auf einem Feld steht** (gleich ob von Hand, über TL-Web oder durch die
  automatische Feldvergabe zugewiesen) **und**
- ihm ein **Schiedsrichter** zugeordnet ist.

Geprüft wird ein **Zustand, kein Moment**: Der Schiedsrichter darf nach der
Feldvergabe dazukommen — durch die Rotation oder von Hand —, dann löst der
nächste Sync-Lauf aus. Umgekehrt genauso. Ein **Aufschlagrichter allein genügt
nicht**; der Zettel gehört dem, der ihn führt.

Vier Dinge, auf die Verlass ist:

- **Höchstens ein Blatt je Spiel.** Ein Feldwechsel oder ein Schiedsrichter-Tausch
  erzeugt keinen zweiten Zettel — Änderungen trägt man von Hand nach. Zwei Zettel
  zum selben Spiel wären am Feld gefährlicher als eine Korrektur mit dem Stift.
- **Ein App-Neustart druckt nichts nach.** Der Vermerk liegt in `gedruckt.json`
  beim Turnier. Ohne ihn sähe der frisch gestartete Host jedes belegte Feld als
  neu zugewiesen — bei zwanzig Feldern zwanzig Blatt.
- **Ein Turnierwechsel beginnt frisch.** Match-IDs gelten nur innerhalb eines
  Turniers.
- **Nur der Turnier-PC druckt.** Ein Ansage-Slave in der zweiten Halle vergibt
  keine Felder und druckt deshalb auch nicht; für Halle 2 richtet man dort einen
  Netzwerkdrucker ein und wählt ihn am Master aus.

**Ohne Schiedsrichterverwaltung bleibt die Automatik stumm.** Ist im Setup „Mit
Schiedsrichtern spielen" aus, kennt bts-light nie einen SR — dann gibt es
bauartbedingt nichts zu drucken. Der Einstellungstext sagt das an Ort und Stelle.

**Wenn der Drucker schweigt**, erscheint app-weit ein gelber Hinweis („Zettel für
Feld 3 …") und der Vorgang steht im Log. Der Druck **wiederholt sich nicht** —
sonst liefe er im Sekundentakt gegen einen ausgeschalteten Drucker. Nach dem
Beheben druckt man den Zettel von Hand nach.

## Drucker einstellen

Im Setup steht der Abschnitt **Schiedsrichterzettel**:

- **Drucker suchen** listet die im System eingerichteten Drucker auf.
- **Windows-Standarddrucker** ist die Vorgabe; sie bleibt richtig, auch wenn
  sich der Standarddrucker später ändert.
- Ein früher gewählter Drucker, den Windows gerade nicht meldet (aus,
  abgezogen), bleibt wählbar — sonst fiele die Einstellung beim nächsten
  Speichern still auf „Standard" zurück.

Gedruckt wird **ohne Dialog** auf dem Rechner, auf dem BTS Light läuft. Das
Papierformat stellt BTS Light selbst ein: **A4 quer**. Das ist keine Kosmetik —
mit der Treibervorgabe kam im Test US-Letter heraus (279 mm breit), und das
275-mm-Raster hätte rechts seine letzte Spalte verloren.

**Druckprobe ohne Papier:** `cargo test --lib druck_nach_pdf -- --ignored
--nocapture` schickt ein Blatt an „Microsoft Print to PDF" und legt
`target/druckprobe.pdf` ab — derselbe Weg wie zum echten Drucker, nur ohne
Baum.

## Was es *nicht* gibt

- **Keinen ausgefüllten Zettel für Spiele ohne Tablet.** Ein von Hand
  eingetragenes Ergebnis hat keine Datenbasis; ein halb ausgefüllter Bogen wäre
  irreführender als keiner. Was es dafür gibt, ist der Vorabzettel oben — leer
  und ehrlich.
- **Kein nachträgliches Bearbeiten** eines abgeschlossenen Zettels.
- **Keine Aufschlagfolge bei Altbestand.** Spiele, die vor dieser Version gezählt
  wurden, haben kein `serve_start`. Der Zettel fällt dann im Doppel auf zwei
  Zeilen (eine je Mannschaft) zurück und sagt das oben dazu.

## Datenschutz

Der Zettel trägt **Spielernamen** und den **Verein**, sofern BTP ihn kennt — das
ist sein Zweck. Der Verein steht dabei **unabhängig** vom turnierweiten Schalter
„Vereine anzeigen" auf dem Blatt: Der Bogen hat eine vorgedruckte Vereinszeile,
und der Verein steht ohnehin auf Aushang und Meldeliste (ADR 0043). Kein
Geburtsjahr, keine Lizenznummer.

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
