# Turnierleitungs-Oberfläche im Browser

Felder vergeben, ohne am Turnier-PC zu stehen: Die Seite läuft auf Tablet,
Telefon oder einem zweiten Rechner — im Hallennetz und, wenn gewünscht, über
das Internet. Mehrere Helfer können gleichzeitig arbeiten.

Diese Datei beschreibt **Einrichtung und Betrieb**. Wie es innen aussieht,
steht in [features/turnierleitung-web.md](features/turnierleitung-web.md)
(Spezifikation), [features/tl-web-panelsystem.md](features/tl-web-panelsystem.md)
(Panels und Profile) und [cloud-relay.md](cloud-relay.md) (Wire-Ebene).

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
  den Aufruf zurück.
- **Das ⋮-Menü an einer Wartelisten-Zeile:** Alles, was seltener gebraucht
  wird, steckt dahinter — **Nachruf** (beide Parteien oder nur eine), der
  **Auto-Vergabe**-Umschalter und der **Hallen-Wähler**. Sichtbar bleibt
  an der Zeile nur, was ständig gebraucht wird; so passt eine Zeile auch
  auf einem schmalen Tablet in eine Zeile statt in vier.
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
  eingeschaltet, erscheint ein eigenes Panel mit den wartenden
  Zähltafelbedienern. Von dort lässt sich jemand **vorziehen** (an den
  Anfang der Schlange), aus der Schlange **entfernen** oder von Hand neu
  **hinzufügen** — dieselben Aktionen wie am Turnier-PC. Ist die Verwaltung
  ausgeschaltet, gibt es das Panel gar nicht.
- **Beendet:** ein eigenes Panel unter der Spielliste, neueste zuerst.
  Aufgabe, kampflos gewertete und disqualifizierte Spiele tragen eine eigene
  Kennzeichnung neben dem Satzstand. Über den Cloud-Weg ist auch diese Liste
  ggf. gekürzt.
- **Was zu sehen ist, bestimmst du:** Jeder Abschnitt der Seite ist ein
  **Panel**, das sich einzeln ausblenden, umsortieren und in der Höhe
  verteilen lässt — gespeichert in einem **Profil** je Gerät. Siehe
  „Panels" und „Profile" weiter unten.

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
für den letzten Punkt eines Satzes, **„Matchball"** (pulsierend — das Feld
wird gleich frei) für den letzten Punkt des Matches. Das ist nur eine
Planungshilfe für die Turnierleitung, keine Wertungslogik, und erscheint
**ausschließlich hier**, nie auf den Hallen-TVs (Court-Monitor/Overview) —
bewusste Scope-Entscheidung vom 20.07.2026, Plan 16.

#### Abzeichen: drei Dringlichkeitsstufen

Alle kleinen Abzeichen an Feldkacheln und Listenzeilen folgen **einer**
Skala — man muss sich nicht je Abzeichen merken, was seine Farbe bedeutet:

| Stufe | Sieht so aus | Was da steht |
|---|---|---|
| **Info** | ruhig, nur umrandet | „Satzball", „Matchball", „manuell einsortiert" — Planungshilfe, nichts zu tun |
| **Warnung** | gelb-orange gefüllt | „⏸ Auto-Vergabe aus", Schiedsrichter-Konflikt — bewusst gesetzt oder nachsehen |
| **Alarm** | rot gefüllt | überfällig, gesperrt, Verletzung, „TL gerufen" — da muss jemand hin |

Info-Abzeichen sind bewusst nur **umrandet** statt ausgefüllt: „Matchball"
ist dadurch auf den ersten Blick von einem Alarm zu unterscheiden, auch
wenn beide rötlich sind. Am Feld-**Streifen** heißt Rot weiterhin
„überfällig", nie „Matchball".

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

### Ein Spiel von der automatischen Feldvergabe ausnehmen

Im **⋮-Menü** jeder Wartelisten-Zeile steht ein Pause-Umschalter
(⏸ Auto-Vergabe). Gedrückt heißt: Dieses Spiel wird von der
**automatischen** Feldvergabe übersprungen, solange die Ausnahme aktiv ist
— ein Abzeichen „⏸ Auto-Vergabe aus" markiert die Zeile zusätzlich. Praktisch,
wenn ein Spieler kurzfristig nicht greifbar ist, das Spiel aber nicht
sofort gewertet oder manuell verschoben werden soll.

**Manuelles Zuweisen bleibt immer möglich** — die Ausnahme betrifft
ausschließlich die Automatik. Ein erneuter Klick nimmt die Ausnahme
zurück; sie räumt sich außerdem von selbst auf, sobald das Spiel gewertet
ist. Bedienbar sowohl hier in TL-Web als auch am Turnier-PC (Felder-
übersicht, Tabelle „Nicht zugewiesene Spiele") — beide Wege zeigen
denselben Stand. Die Ausnahme überlebt einen Neustart des Turnier-PCs
(`excluded-matches.json`, turniergebunden wie die Schiedsrichter-
Einteilung).

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

### Panels: was auf dieser Seite steht

Die Seite besteht aus neun **Panels** — Felder, Aufgaben (Walkover),
Zähltafel-Warteschlange, Schiedsrichter, die vier Wartelisten-Abschnitte
(„In Vorbereitung gerufen", „Spielbereit", „Noch nicht bereit", „Ohne
Hallenzuordnung") und „Beendet". Jedes trägt dieselbe Kopfzeile: Titel,
Anzahl, und rechts ein **Auge** zum Aus- und Einblenden.

- **Ausblenden ist dauerhaft** (nicht nur zugeklappt): Ein ausgeblendetes
  Panel belegt keinen Platz mehr und bleibt nach einem Neuladen der Seite
  ausgeblendet. Wer keine Schiedsrichter einteilt, wird den Abschnitt
  dadurch wirklich los.
- **Reihenfolge ändern**: Am Kopf jedes Panels sitzt ein Verschiebe-Griff
  (Kreuz-Pfeile — bewusst ein anderes Symbol als der ⠿-Griff, mit dem man
  einzelne *Zeilen* zieht). Ziehen oder mit den Pfeiltasten verschieben.
- **Höhe verteilen**: Zwischen je zwei sichtbaren Panels sitzt ein
  Trennsteg. Ziehen gibt dem einen Panel mehr Platz und dem anderen
  weniger; **Doppeltipp** stellt die gleichmäßige Verteilung wieder her.
  Kein Panel lässt sich auf null ziehen. Ist ein Panel dazwischen
  ausgeblendet, greift der Steg automatisch das nächste sichtbare.
- **„Felder" bleibt oben bzw. links** — das ist die feste Ankerposition
  der Seite. Aus-/Einblenden und Höhe verteilen geht trotzdem, nur
  umsortieren nicht. Ob die Spielliste rechts daneben oder darunter
  steht, entscheidet weiterhin die Einstellung **Spielliste
  rechts/darunter** (siehe Profile).

### Profile (Knopf im Kopf)

Alles, was diese Seite unterschiedlich aussehen lässt, steckt in einem
**Profil**: welche Panels sichtbar sind, in welcher Reihenfolge, wie hoch,
und die Anzeige-Häkchen weiter unten. Ein Tablet in der Hand braucht etwas
anderes als ein Wandmonitor — statt an jedem Gerät einzeln zu schrauben,
legst du einmal ein „Tablet"- und ein „Wandmonitor"-Profil an und wählst
am jeweiligen Gerät seins.

- **Anlegen/Bearbeiten/Löschen** über den **Profile**-Knopf im Kopf.
  Löschen fragt nach und ist nicht rückgängig zu machen.
- **Für dieses Gerät wählen** — die Wahl hängt am Gerät (nicht am Browser)
  und übersteht ein Neuladen, im Hallennetz wie über die Cloud.
- **Als Standard markieren** — welches Profil ein Gerät bekommt, das noch
  keins gewählt hat.
- Profile gelten **turnierübergreifend** und bleiben bei einem Neustart
  des Turnier-PCs erhalten (sie stehen in der Konfiguration, nicht in den
  Turnierdaten).
- Wird ein Profil gelöscht, das ein Gerät gerade nutzt, fällt dieses Gerät
  beim nächsten Abruf **von selbst auf das Standardprofil** zurück — ohne
  Fehlermeldung.
- Ändern zwei Geräte gleichzeitig dasselbe Profil, gewinnt die zuletzt
  gespeicherte Fassung. Keine Warnung, kein Konfliktdialog — in der Praxis
  richtet man Profile einmal vor dem Turnier ein, nicht gleichzeitig zu
  zweit.
- **Das Profil gilt verbindlich.** Anders als früher kann ein Gerät
  einzelne Häkchen nicht mehr für sich überstimmen — wer es anders will,
  legt ein eigenes Profil an. Dafür sieht man an jedem Gerät auch
  wirklich das, was im gewählten Profil steht.
- Ohne angelegtes Profil läuft die Seite auf einem eingebauten Standard
  (alle Panels sichtbar, Voreinstellungen wie unten). Die erste eigene
  Änderung macht daraus automatisch ein echtes Profil.

Die Anzeige-Häkchen im Profil-Editor:

- **Spielnummer zeigen** (Standard: an) — die Zahl ganz links in der Liste.
- **Nationen zeigen** (Standard: **aus**) — die **Flagge** neben jedem
  Namen, dieselben Bilder wie auf dem Court-Monitor. Zu einer Nation ohne
  Flaggendatei erscheint das Kürzel. Hilfreich bei internationalen
  Turnieren.
- **Spielliste rechts / darunter** (Standard: rechts) — auf einem
  schmaleren Tablet steht die Liste lieber unter den Feldern als daneben.
- **Disziplin/Klasse, Runde, Gruppe zeigen** (Standard: alle drei an) —
  einzeln abschaltbar in der Meta-Zeile der Warteliste; Feldkacheln und
  „Beendet" zeigen sie unverändert weiter.

Daneben, nicht Teil der Profile, aber am selben Kopfbereich: **Automatik**
an- und abschalten — der Schalter oben rechts.

#### Vereine (Vereinsname/-logo)

Im Profil-Editor stehen **„Vereinsnamen zeigen"** und **„Vereinslogos zeigen"**.
Die **Voreinstellung** kommt aus dem Setup unter **„Vereine anzeigen"** (die
turnierweite Wahl, die zugleich die Tablet-Spielzettel steuert); ein Profil
kann sie überschreiben. Standard turnierweit: **aus**.

- **Vereinsnamen zeigen** — der Vereinsname in einer eigenen kleinen Zeile
  **unter** dem Spielernamen (mit Wappen davor, wenn Logos an sind).
- **Vereinslogos zeigen** — das Vereinswappen aus dem badhub-Bestand (dieselbe
  Quelle wie die Siegerliste). Ist **nur das Logo** an (ohne Name), steht das
  Wappen kompakt **vor** dem Namen, direkt hinter der Nation. Zu einem Verein
  ohne hinterlegtes Logo bleibt es beim Namen.

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
(`localStorage`). Nicht zu verwechseln mit den Stegen **zwischen den
Panels** innerhalb der Liste (siehe „Panels") — dieser hier trennt die
beiden großen Bereiche voneinander.

**Doppeltipp/Doppelklick auf den Steg** stellt die automatische,
bedarfsgerechte Aufteilung der aktuellen Anordnung wieder her. Grenzen
werden geklammert: Die Liste fällt nie unter ihr Mindestmaß (320 px Breite
bzw. `--liste-min` Höhe), die Felder behalten mindestens eine Kachel —
auch nach einem Fenster-Resize.

Gesprochen wird nie auf diesem Gerät: Die Seite beauftragt die Ansage, und
gesprochen wird sie dort, wo die Anlage hängt. Hört in der Zielhalle gerade
kein Ansage-Gerät zu, sagt die Seite das ausdrücklich.

### Schiedsrichter einteilen

Spielt das Turnier mit Schiedsrichtern (Einstellungen → Schiedsrichter),
zeigt die Seite zusätzlich:

- **Abschnitt „Schiedsrichter"** unter der Zähltafel-Warteschlange: die
  Liste in Rotationsreihenfolge mit Dienst-Marke, Pause-Knopf, Zieh-Griff
  zum Umsortieren (Drag & Drop, auch auf dem Tablet — seit 14.08.2026,
  ersetzt die frühere Pfeil-Bedienung) und der Zahl der bisherigen Einsätze.
  Ein Tipp auf die Zahl öffnet die Pflege: Stammverein, gesperrte Vereine,
  gesperrte Spieler und die Einsatz-Liste im Detail. Auch bei eingeklappter
  Liste steht in der Kopfzeile, wer als Nächstes zugeteilt würde.
- **An jeder belegten Feld-Kachel** „SR: … · AR: …" samt Warnfarbe, wenn ein
  Konflikt besteht (Kategorie „Verein" oder „Person" — der Grund bleibt am
  Turnier-PC), plus den Knopf **einteilen** für die Auswahl je Dienst.

Eine Zuweisung mit Konflikt wird **ausgeführt** und nur gekennzeichnet; die
Turnierleitung entscheidet. Steht in BTP schon ein Schiedsrichter am Spiel,
gilt dieser — die Auswahl hier wirkt dann nicht.

Sperrlisten, Verein und Einsatz-Liste stehen **nicht** im Zustand, den alle
gekoppelten Geräte bekommen: Sie kodieren persönliche Beziehungen und werden
erst beim Öffnen der Pflege gezielt und mit dem Geräte-Zugang abgerufen
(`/tl/api/officials/{id}`, gleiches Muster wie der Punktverlauf).

**Im Cloud-Betrieb** funktioniert die Schiedsrichter-Bedienung erst, wenn der
Relay auf badhub.de die neuen Aktionen kennt (Deploy vor dem Client-Release);
im Hallennetz sofort.

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
