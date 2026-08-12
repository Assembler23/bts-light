# Turnierleitungs-Oberfläche im Browser

Felder vergeben, ohne am Turnier-PC zu stehen: Die Seite läuft auf Tablet,
Telefon oder einem zweiten Rechner — im Hallennetz und, wenn gewünscht, über
das Internet. Mehrere Helfer können gleichzeitig arbeiten.

Diese Datei beschreibt **Einrichtung und Betrieb**. Wie es innen aussieht,
steht in [features/turnierleitung-web.md](features/turnierleitung-web.md)
(Spezifikation) und [cloud-relay.md](cloud-relay.md) (Wire-Ebene).

## Einrichten

1. In bts-light auf **Turnierleitung** gehen.
2. **Freischalten** — die Oberfläche ist ab Werk aus.
3. Namen eintragen („Tablet Meeting Point") und **Koppeln**.
4. Den QR-Code mit dem Gerät scannen. Die Seite öffnet sich angemeldet;
   einen Code muss niemand abtippen.

**Der Zugang ist genau einmal zu sehen.** Danach lässt er sich nicht mehr
anzeigen — auch nicht für Sie. Geht ein Gerät verloren, entziehen Sie ihm
den Zugang und koppeln neu. Das ist der kürzere Weg als ein Zugang, der
dauerhaft in einer Liste steht und irgendwann jemand anderes liest.

Klappt das Scannen nicht, steht die Adresse unter dem QR-Code auch als Text —
mit einem Kopier-Knopf daneben, etwa um sie am selben Rechner in einem
Browser-Tab zu öffnen oder per Messenger auf das Gerät zu bringen.
Sie enthält den Zugang hinter einem `#` — dieser Teil wird von keinem
Browser an einen Server geschickt und taucht deshalb in keinem Protokoll auf.

**Vor dem Koppeln die Übertragung starten.** Läuft sie nicht, geht die
Adresse ins Leere, das Gerät übernimmt den Zugang nie — und der war nur
dieses eine Mal zu sehen. Die Seite warnt in diesem Fall.

Der Internet-Weg (zweiter QR-Code) setzt den Verbindungsmodus Cloud oder
LAN+Cloud voraus.

## Im Betrieb

- **Zuweisen:** Spiel antippen, dann Feld antippen. Oder ziehen. Beides
  führt zum selben Ergebnis; bei abgebrochenem Ziehen bleibt die Auswahl
  stehen. Auf Touch-Geräten zieht man am **Griff** (⠿) an der Zeile bzw.
  Feldkachel — ein Tipp irgendwo sonst auf der Zeile bleibt ein normaler
  Tipp, und die Liste lässt sich weiter ganz normal wischen. Antippen-dann-
  Antippen funktioniert dort unverändert und ohne den Griff.
- **Umhängen:** Spiel auf dem Feld antippen, dann das Zielfeld. Das ist
  **ein** Schreibvorgang nach BTP — es gibt keinen Moment, in dem das Spiel
  auf keinem Feld steht.
- **Vom Feld nehmen:** über das Band oben. Läuft schon ein Spielstand, wird
  vorher gefragt.
- **Ergebnis eintragen:** über das Band oben, für **jedes** gewählte Spiel —
  auch eines, das noch in der Warteliste steht und nie auf einem Feld war
  (jemand hat mündlich oder auf einem Zettel abgerechnet). Ein Dialog mit
  Satzfeldern öffnet sich, bei einem Feld-Spiel mit dem Live-Stand vorbelegt,
  sonst leer. Lehnt der Turnier-PC den Stand ab (unplausible Sätze, ein
  Folgespiel hängt bereits daran), steht die Meldung **im Dialog** — nicht
  als flüchtiger Toast, der weg wäre, während noch getippt wird.
- **Aufrufen:** Der Knopf am Feld löst den zweiten bzw. dritten Aufruf aus.
  Die Stufe zählt der Turnier-PC — alle Geräte, auch die Desktop-App, zeigen
  dieselbe Zahl.
- **In Vorbereitung rufen:** das **Megafon** an der Zeile. Es ist ein
  Umschalter — hervorgehoben heißt „ist gerufen", noch einmal tippen nimmt
  den Aufruf zurück. Daneben der **Nachruf** (beide Parteien oder nur eine).
- **Was steht wo:** Jedes Spiel nennt seine **Klasse** in der gewohnten
  Schreibweise — `HE-C`, `HD-D` — vor Auslosung und Runde. Turniere
  benennen ihre Gruppen frei, und „Gruppe 6" allein verrät nicht, worum es
  geht. Fehlt eine der beiden Hälften, steht die andere für sich.
- **Die Felder bleiben immer vollständig sichtbar**, nur die Spielliste
  scrollt für sich (Wunsch vom 10.08.2026 nach dem Turniertest): Auch ein
  Spiel von ganz unten lässt sich noch auf ein Feld ziehen oder tippen. Bei
  sehr vielen Feldern verkleinern sich die Kacheln dafür stufenweise
  (kleinere Abstände, dann kleinere Schrift), bevor überhaupt gerollt wird.
- **Zähltafel-Warteschlange:** Ist die Zähltafel-Verwaltung in bts-light
  eingeschaltet, erscheint rechts ein eigener aufklappbarer Abschnitt mit
  den wartenden Zähltafelbedienern. Von dort lässt sich jemand **vorziehen**
  (an den Anfang der Schlange), aus der Schlange **entfernen** oder von Hand
  neu **hinzufügen** — dieselben Aktionen wie am Turnier-PC. Ist die
  Verwaltung ausgeschaltet, bleibt der Abschnitt unsichtbar.
- **Beendet:** ein zugeklappter Abschnitt unter der Spielliste, neueste
  zuerst. Aufgabe, kampflos gewertete und disqualifizierte Spiele tragen
  eine eigene Kennzeichnung neben dem Satzstand. Über den Cloud-Weg ist auch
  diese Liste ggf. gekürzt.

### Die Farbe eines Feldes

Jede Feldkachel trägt links einen Streifen in der Farbe ihres Zustands. Aus
zehn Metern erkennbar, ohne dass die Namen an Kontrast verlieren:

| Farbe | Bedeutung |
|---|---|
| — (grau, gestrichelt) | frei |
| Blau | aufgerufen, es geht gleich los |
| **Rot** (ganze Kachel getönt) | aufgerufen, aber **noch kein einziger Punkt** — und die eingestellte Zeit ist um. Da muss jemand hin. |
| Grün | es wird gezählt |
| Violett | beendet, BTP hat das Feld noch nicht freigegeben |

Ab wann „überfällig" gilt, stellst du in bts-light unter **Aufruf-Timer**
ein („Feld färbt sich rot, wenn nach … Minuten noch kein Punkt gefallen
ist", Standard 5). Die Einstellung gilt für alle Geräte — sonst leuchtete
eine Halle rot und die andere nicht. Sie wirkt unabhängig davon, ob der
Aufruf-Timer selbst eingeschaltet ist.

Die Farbe steht nie allein: Satzstand und Uhrbeschriftung („im Spiel seit"
gegenüber „auf dem Feld seit") sagen dasselbe in Worten.

Zusätzlich zum Zustands-Streifen bekommt eine laufende Kachel bei Satz- oder
Matchball einen zweiten, umlaufenden Rahmen plus Abzeichen: **„Satzball"**
(gelb) für den letzten Punkt eines Satzes, **„Matchball"** (rot, pulsierend
— das Feld wird gleich frei) für den letzten Punkt des Matches. Das ist nur
eine Planungshilfe für die Turnierleitung, keine Wertungslogik, und
erscheint **ausschließlich hier**, nie auf den Hallen-TVs (Court-Monitor/
Overview) — bewusste Scope-Entscheidung vom 20.07.2026, Plan 16. Die
Streifenfarbe bleibt davon unberührt: Rot heißt am Streifen weiterhin
„überfällig", nicht „Matchball".

### Spielort

**Pflegst du in BTP die Spalte „Spielort", übernimmt bts-light sie.** Die
Halle steht dann schon an jedem wartenden Spiel, ohne dass jemand etwas
eintragen muss (BTP-Feld `Match.LocationID`).

Für Turniere, die das nicht tun, steht an jeder Zeile der Warteliste ein
kleiner Hallen-Wähler („Ky", „Lu" — die Kürzel deiner Hallen, voller Name
im Tooltip). Damit legst du fest, **wo** ein Spiel stattfinden soll, ohne
es schon auf ein Feld zu legen; „–" nimmt die Festlegung zurück. Was du
von Hand setzt, gilt vor dem, was in BTP steht — du disponierst ja um.

Eine von Hand gesetzte Halle **überlebt einen Neustart** des Turnier-PCs
(`spielorte.json` neben der Konfiguration).

BTP übernimmt diese Hand-Festlegung nicht: Ein Schreibversuch per
`SENDUPDATE` wird von BTP zwar mit `Result=1` beantwortet, der Wert aber
verworfen (gemessen 10.08.2026) — die Festlegung wirkt deshalb bewusst
nur in bts-light selbst und im Liveticker.

Ein von Hand gesetzter Ort wirkt an drei Stellen:

- **Hallenfilter** dieser Seite — das Spiel erscheint in seiner Halle
  statt unter „ohne Hallenzuordnung".
- **Vergabe** — das Spiel gehört dann in diese Halle. Ein Feld der
  **anderen** Halle ist trotzdem wählbar (seit 11.08.2026): Vor dem
  Schreiben kommt eine Sicherheitsabfrage („… gehört nach X — wirklich
  auf Feld Y in Z legen?") — manchmal muss ein Spiel bewusst in die
  andere Halle geholt werden, meist ist es aber ein Tipp daneben.
- **Liveticker**: `badhub.de/live?display=next&halle=…` zeigt es. Bisher
  blieb dieser Filter leer, sobald ein Turnier seine Aufrufe über BTP
  statt über bts-light machte. Das Spiel gilt dadurch **nicht** als
  aufgerufen — es steht kein „vor X Min gerufen" daran.

Legt eine Disziplin/Klasse→Halle-Regel den Ort schon fest, erscheint kein
Wähler: Die Regel bindet auch die Vergabe, eine abweichende Handzuweisung
käme nie aufs Feld. Die Festlegung gilt für den laufenden Betrieb und
endet mit dem Stoppen der Übertragung.

### Anordnung wie in der Halle

Statt Feldern in Formularreihenfolge lässt sich für jede Halle ein
**Raster** hinterlegen — dieselbe Anordnung, in der die Felder tatsächlich
stehen. Eingestellt wird das je Halle über das **Zahnrad** an der
Hallenüberschrift in der Felderübersicht der App (Spalten, Start-Ecke,
Nummerierungsrichtung reihenweise/horizontal oder spaltenweise/vertikal,
Zick-Zack-/Schlangen-Nummerierung). Es ist eine **Host-Einstellung**: Alle
Geräte — App wie Turnierleitungs-Oberfläche — zeigen dasselbe Raster, sonst
meinte „das Feld links unten" auf jedem Tablet etwas anderes. Eine Halle
ohne hinterlegtes Raster erscheint weiterhin in der bisherigen
Fließ-Darstellung.

Details zum Datenmodell und zur Vergleichsregel für Hallennamen:
[features/feld-raster.md](features/feld-raster.md).

### Anzeige (Klappmenü im Kopf)

Drei Einstellungen, die **je Gerät** gelten und dort gespeichert bleiben —
der eine Helfer sucht nach der Spielnummer aus dem Papierplan, der andere
kann damit nichts anfangen:

- **Spielnummer zeigen** (Standard: an) — die Zahl ganz links in der Liste.
- **Nationen zeigen** (Standard: **aus**) — die **Flagge** neben jedem
  Namen, dieselben Bilder wie auf dem Court-Monitor. Zu einer Nation ohne
  Flaggendatei erscheint das Kürzel. Hilfreich bei internationalen
  Turnieren.
- **Spielliste rechts / darunter** (Standard: rechts) — auf einem
  schmaleren Tablet steht die Liste lieber unter den Feldern als daneben.
  Reine Anzeigefrage, keine Turniereinstellung: Gerät A kann „rechts"
  zeigen, Gerät B gleichzeitig „darunter".
- **Disziplin/Klasse, Runde, Gruppe zeigen** (Standard: alle drei an) —
  einzeln abschaltbar in der Meta-Zeile der Warteliste; Feldkacheln und
  „Beendet" zeigen sie unverändert weiter.

Daneben, nicht Teil dieses Menüs, aber am selben Kopfbereich: **Automatik**
an- und abschalten — der Schalter oben rechts.

#### Vereine (Vereinsname/-logo)

Anders als die obigen Schalter ist die **Vereins-Anzeige turnierweit
zentral**, nicht je Gerät: Sie wird einmal im Setup unter **„Vereine
anzeigen"** gesetzt und gilt dann für die Turnierleitungssicht **und** die
Tablet-Spielzettel gleichermaßen. Zwei getrennte Optionen:

- **Vereinsnamen anzeigen** (Standard: **aus**) — der Vereinsname klein
  hinter jedem Spielernamen.
- **Vereinslogos anzeigen** (Standard: **aus**) — das Vereinswappen davor,
  aus dem badhub-Bestand (dieselbe Quelle wie die Siegerliste). Zu einem
  Verein ohne hinterlegtes Logo bleibt es beim Namen.

Im Hallennetz (LAN) holt der Turnier-PC die Logos (funktioniert ohne Internet
am Anzeigegerät und matcht Vereinsnamen unscharf). Im Cloud-Modus lädt die
Seite die Logos direkt über den öffentlichen badhub-Logo-Resolver
(`/api/v1/club-logo`); dort zählt der **exakte** Vereinsname, ein Verein mit
Zusatz wie „(Berlin)" wird also nur getroffen, wenn er in badhub genauso
heißt. Fehlt ein Logo, bleibt es beim Namen. Der **Verein** ist wie die
Nationalität ein bewusst zuschaltbares, standardmäßig ausgeschaltetes
Anzeige-Feld (Datenschutz).

### Punktverlauf ansehen

Der **📈-Knopf** an einer belegten Feldkachel und an Zeilen der
Beendet-Liste öffnet den **Punktverlauf** des Spiels: je Satz ein
Liniendiagramm (x = Ballwechsel, y = Punkte, eine Linie je Partei).
Bei laufenden Spielen wächst die Kurve mit. Der Knopf erscheint nur,
wenn ein Tablet das Spiel gezählt hat — Papier-Ergebnisse haben keinen
Verlauf. Details: [punktverlauf.md](punktverlauf.md).

### Aufteilung Felder/Spielliste ziehen

Zwischen Feldern und Spielliste sitzt ein **Trennsteg** (kleine Pille in
der Lücke). Ziehen verschiebt die Grenze: nebeneinander die **Breite** der
Liste, gestapelt („Spielliste darunter") ihre **Höhe**. Das Maß gilt
**je Gerät** und je Anordnung getrennt und bleibt gespeichert
(`localStorage`, wie die übrigen Anzeige-Einstellungen).

**Doppeltipp/Doppelklick auf den Steg** stellt die automatische,
bedarfsgerechte Aufteilung der aktuellen Anordnung wieder her. Grenzen
werden geklammert: Die Liste fällt nie unter ihr Mindestmaß (320 px Breite
bzw. `--liste-min` Höhe), die Felder behalten mindestens eine Kachel —
auch nach einem Fenster-Resize.

Gesprochen wird nie auf diesem Gerät: Die Seite beauftragt die Ansage, und
gesprochen wird sie dort, wo die Anlage hängt. Hört in der Zielhalle gerade
kein Ansage-Gerät zu, sagt die Seite das ausdrücklich.

## Grenzen, die im Betrieb auffallen

- **Acht Geräte gleichzeitig.** Das neunte wird abgewiesen; ein
  geschlossener Tab gibt seinen Platz nach einer Minute selbst frei. Die
  Liste der Kopplungen darf länger sein — alte Kopplungen blockieren keinen
  Platz.
- **Ergebnisse korrigieren geht nur, solange nichts daran hängt.** Der Stift
  an einer Zeile in **Beendet** öffnet denselben Dialog wie „Ergebnis
  eintragen", mit dem zuletzt gemeldeten Stand vorbelegt. Ein bereits
  gewertetes Spiel lässt sich überschreiben, wenn kein Folgespiel existiert
  (Finale, Gruppenspiel). Sobald der Sieger im nächsten Spiel steht, wird
  abgelehnt — die Ablehnung erscheint im Dialog, nicht als Toast — was BTP
  beim Überschreiben mit dem Turnierbaum macht, ist noch nicht abschließend
  geklärt ([btp_protocol.md](btp_protocol.md)). Bis dahin: in BTP von Hand.
- Über den **Cloud-Weg** ist die Warteliste auf 40 Spiele je Halle gekürzt
  (im Hallennetz sind es 120). Der Zustand geht bei jedem Ballwechsel neu
  über die Leitung; die volle Liste wäre ein Dauerstrom über Mobilfunk. Was
  fehlt, sagt die Seite.

## Wenn etwas nicht geht

| Was Sie sehen | Was dahintersteckt |
|---|---|
| „Turnier-PC ist nicht verbunden" | Die Übertragung läuft nicht oder die Verbindung ist ab. Die Seite zeigt weiter den letzten Stand und meldet sich, sobald es weitergeht. |
| „Zugang gilt nicht mehr" | Das Gerät wurde entkoppelt — oder die Oberfläche ist abgeschaltet. Die Seite wirft den Zugang **nicht** weg, sondern versucht es weiter; ist es ein Irrtum, kommt sie von selbst zurück. |
| „Zu viele Geräte" | Mehr als acht Geräte haben die Seite offen. Ein nicht benutztes schließen; der Platz wird nach einer Minute frei. |
| „Feld wurde gerade von jemand anderem belegt" | Zwei Helfer waren gleichzeitig dran. Genau einer gewinnt — so ist es gedacht. Die Ansicht springt auf den echten Stand. |
| Die Seite ist leer, obwohl Spiele laufen | Kein Zugang übernommen (falscher Link) oder die Oberfläche ist aus. Unter **Turnierleitung** nachsehen. |

## Abschalten

Der Schalter unter **Turnierleitung** wirkt sofort — im Hallennetz wie über
das Internet. Die Kopplungen bleiben erhalten: Ein versehentlicher Klick
bedeutet nicht, dass alle Tablets neu gescannt werden müssen.

Im Notfall lässt sich die Oberfläche auch **serverseitig** abschalten, ohne
die übrigen Dienste anzufassen: `BTS_RELAY_TL=off` am Relay. Genau dieses
eine Wort schaltet ab.
