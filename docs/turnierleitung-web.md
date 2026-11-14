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

Klappt das Scannen nicht, steht die Adresse unter dem QR-Code auch als Text.
Ein Klick darauf öffnet sie am selben Rechner im Browser; mit dem
Kopier-Knopf daneben bringen Sie sie etwa per Messenger auf das Gerät.
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
  dieselbe Zahl. Im Regelfall endet der Knopf nach dem dritten Aufruf und
  verschwindet, sobald Punkte gefallen sind. Mit der Profil-Option
  **„Aufrufe unbegrenzt"** (Gruppe „Felder" im Profil-Editor, seit
  17.08.2026) bleibt er dagegen immer da — auch bei laufendem Spiel und
  über den dritten Aufruf hinaus („Erneut aufrufen"). Ab dem vierten
  Aufruf erklingt die schlichte Feld-Ansage ohne Stufenwort; „Dritter und
  letzter Aufruf" wird nicht wiederholt. Bei einem **laufenden** Spiel
  (Punkte gefallen) spricht das Ansage-Gerät grundsätzlich ohne
  Stufenwort — „Zweiter Aufruf" mitten ins Spiel wäre absurd. Der dritte
  Aufruf bleibt der letzte *reguläre*: Sein Knopf heißt weiter „Letzter
  Aufruf", auch wenn danach „Erneut aufrufen" möglich ist.
- **Spiel komplett ansagen** (seit v0.9.370): Im ⋯-Menü jeder belegten
  Feld-Kachel steht **immer** „📣 Spiel komplett ansagen" — egal, ob das
  Spiel schon läuft, ob der letzte Aufruf gesprochen ist oder die
  Aufruf-Knöpfe gerade fehlen. Es erklingt noch einmal die vollständige
  Ansage wie beim ersten Aufruf (Feld, Disziplin, Paarung und — je nach
  Ansage-Einstellungen — Schiedsrichter und Bedienung), ohne „Zweiter" oder
  „Letzter Aufruf". Das ist **kein** Aufruf: Die Aufruf-Zählung bleibt
  stehen, und du kannst beliebig oft wiederholen. Ein älterer Turnier-PC
  kennt die Ansage noch nicht, dann fehlt der Eintrag.
  <!-- pruef: "Spiel komplett ansagen" in src-tauri/assets/tl.html -->
- **Bitte anfangen:** Steht ein Spiel auf dem Feld, ohne dass ein Punkt
  fällt, löst „Bitte anfangen" im ⋯-Menü der Kachel die Ansage
  **„Feld X. Bitte mit dem Spielen beginnen."** aus. Das ist **kein**
  Aufruf: Die Aufruf-Zählung bleibt stehen, das Abzeichen springt nicht,
  und du kannst beliebig oft nachfassen. Der Knopf verschwindet, sobald
  gezählt wird.
- **Bedienung nachrufen:** Ist dem Feld ein Zähltafelbediener zugewiesen,
  ruft „Bedienung nachrufen" im ⋯-Menü der Kachel ihn aus
  („Feld 3. Meier, bitte als Tabletbedienung melden."). Ab dem zweiten Mal
  mit Stufenwort. Das ist **kein** Spieler-Aufruf: Die Aufruf-Zählung der
  Spieler bleibt stehen. Ohne zugewiesene Bedienung gibt es den Knopf nicht.
- **In Vorbereitung rufen:** der oberste Eintrag im **⋮-Menü** der Zeile,
  **📣 In Vorbereitung rufen**. Ist das Spiel gerufen, heißt derselbe
  Eintrag **📣 Aufruf zurücknehmen**; an der Zeile steht dann die Marke
  „gerufen". Bis v0.9.323 war das ein Megafon-Knopf direkt an der Zeile.
  Seit v0.9.331 wird jeder neue Aufruf **angesagt** („In Vorbereitung …
  Bitte in Halle X"), sofern ein Ansage-Gerät läuft; ein erneuter Aufruf in
  dieselbe Halle nicht noch einmal. Hat die Halle die Ansage überhört, spielt
  **Erste Ansage wiederholen** (⋮-Menü, Gruppe „Nachruf") sie unverändert
  noch einmal ab — das zählt **nicht** als Nachruf.
  <!-- pruef: "Erste Ansage wiederholen" in src-tauri/assets/tl.html -->
  Nachrufe danach beginnen bei „Zweiter
  Aufruf".
  <!-- pruef: "In Vorbereitung rufen" in src-tauri/assets/tl.html -->
  <!-- pruef: "Aufruf zurücknehmen" in src-tauri/assets/tl.html -->
- **Was an einer Wartelisten-Zeile sichtbar ist:** rechts nur das
  **⋮-Menü**, links der Zieh-Griff ⠿. Mit dem Profil-Schalter
  „Verschiebe-Knöpfe" kommen die vier Knöpfe ⤒ ↑ ↓ ⤓ vor das Menü (siehe
  [Spiele in der Warteliste umsortieren](#spiele-in-der-warteliste-umsortieren)).
- **Das ⋮-Menü an einer Wartelisten-Zeile:** Ganz oben stehen die beiden
  häufigsten Handgriffe, **📣 In Vorbereitung rufen** und **↑ An den Anfang
  der Spielliste**, danach — im Schiedsrichter-Betrieb — **Schiedsrichter
  einteilen** (siehe [Schiedsrichter einteilen](#schiedsrichter-einteilen)),
  „Ergebnis eintragen", **🖨 Zettel (leer)**, der
  **Auto-Vergabe**-Umschalter, der **Hallen-Wähler**, das **Wunschfeld**,
  **Hervorheben** (Zeilenfarbe wie in BTP) und — an einem gerufenen Spiel —
  **Nachruf** (beide Parteien oder nur eine; dazu **Erste Ansage
  wiederholen**, siehe unten). An der Zeile selbst steht so
  wenig wie möglich; so passt eine Zeile auch auf einem schmalen Tablet in
  eine Zeile statt in vier.
  <!-- pruef: "↑ An den Anfang der Spielliste" in src-tauri/assets/tl.html -->
- **Das Menü schließt nach der Wahl von selbst** (seit v0.9.324) — an der
  Wartelisten-Zeile, an der Feldkachel (⋯) und am beendeten Spiel. Ein Tipp
  auf einen Eintrag führt die Aktion aus und klappt das Menü zu; eine
  Aktion kostet damit genau zwei Tipps: Menü auf, Eintrag. Offen bleibt es
  nur, solange die Wahl noch nicht getroffen ist: Beim **Hallen-Wähler**
  und beim **Wunschfeld** schließt es erst, wenn ein Feld bzw. eine Halle
  ausgewählt ist, nicht schon beim Aufklappen der Liste. Eine Farbe unter
  **Hervorheben** ist mit einem Tipp gewählt und schließt sofort. Ein Tipp
  auf eine Überschrift oder freie Fläche im Menü schließt nicht; ein Tipp
  auf ⋮ (oder ein anderes Menü) schließt wie bisher.
- **Hervorheben** (seit v0.9.277, Spec
  [`features/tl-zeilenfarbe.md`](features/tl-zeilenfarbe.md)): Im ⋮-Menü
  einer Zeile stehen **„keine" und sechs Farbfelder** — Gelb, Pink, Orange,
  Blau, Grün, Lila, in derselben Reihenfolge und denselben Tönen wie BTPs
  Kontextmenü „Hervorheben". Ein Tipp schreibt die Farbe **nach BTP**; die
  Zeile ist sofort eingefärbt, und im BTP-Planer erscheint dieselbe Farbe.
  Umgekehrt zeigt die Liste jede Farbe, die jemand im Planer setzt (auch an
  offenen Paarungen). Läuft das Spiel, trägt die **Feldkachel** neben dem
  Feldnamen einen Farbpunkt statt eines Volltons. Was eine Farbe bedeutet,
  vereinbart das Team — bts-light und BTP geben ihr keine Bedeutung.
  **Zusammenspiel mit dem Vorbereitungs-Aufruf:** Der Aufruf färbt ein
  Spiel in BTP gelb und nimmt das Gelb beim Ruf aufs Feld wieder zurück.
  Ein Spiel, das schon eine Handfarbe trägt, lässt der Aufruf in Ruhe; und
  färbt jemand ein gerufenes Spiel um, bleibt die neue Farbe auch nach dem
  Aufruf stehen (ADR 0056). Nur Hand-**Gelb** an einem gerufenen Spiel kann
  der Aufruf nicht von seinem eigenen Gelb unterscheiden — es verschwindet
  mit dem Ruf aufs Feld. Fehlt die Gruppe im Menü, ist der Turnier-PC noch
  auf einer älteren Version.
- **🖨 Zettel (leer)** druckt den **Vorabzettel** des Spiels: derselbe
  Bogen wie der Archivzettel, aber mit leerem Raster — zum Mitgeben an
  den Schiedsrichter, der ihn von Hand führt. Kopf, Namen und Verein sind
  vorgedruckt; Feld, Halle und Besetzung nur, soweit sie schon feststehen.
  Anders als beim Zettel der Beendet-Liste gibt es hier **keine**
  Aufzeichnungs-Schranke — dass noch nichts gezählt wurde, ist ja der
  Grund für das leere Blatt. Gedruckt wird über das Gerät, auf dem TL-Web
  läuft. Details: [schiedsrichterzettel.md](schiedsrichterzettel.md).
- **Was steht wo:** Jedes Spiel nennt seine **Klasse** in der gewohnten
  Schreibweise — `HE-C`, `HD-D` — vor Auslosung und Runde. Turniere
  benennen ihre Gruppen frei, und „Gruppe 6" allein verrät nicht, worum es
  geht. Fehlt eine der beiden Hälften, steht die andere für sich.
  Jugendklassen heißen wie auf dem Aushang `JE-U15`, `ME-U11`, `JD-U17`,
  `MD-U13`, `MX-U19` — BTP kennt nur Herren und Damen, die Jugend erkennt
  bts-light an der Altersklasse „U" + Zahl.
  <!-- pruef: "Jungeneinzel" in src-tauri/assets/tl.html -->
- **Bereitschaft als Farbe statt Text** (seit 17.08.2026): Die früheren
  Hinweistexte („nicht bereit", „X spielt gerade", „pausiert noch bis …",
  „ohne Halle") machten die Liste unübersichtlich und sind weg. Stattdessen:
  Ein Name mit **rotem** Hintergrund steht gerade auf einem Feld, einer mit
  **orangem** Hintergrund ist noch in seiner Mindestpause — daneben zählt
  eine **Eieruhr** die verbleibende Wartezeit sekündlich herunter. Wer es
  in Worten will: Der Mauszeiger (bzw. langes Tippen) auf dem Namen zeigt
  „spielt gerade" bzw. „pausiert noch bis …". Ein Spiel ohne
  Hallenzuordnung erkennst du schlicht am **fehlenden Hallen-Kürzel**.
- **Wunschfeld** (seit v0.9.262): Im ⋯-Menü eines Spiels lässt sich ein
  **Feld** wählen. Die Automatik legt dieses Spiel dann nur dorthin — und hält
  das Feld frei, sobald das Spiel dran ist. Gedacht für Endspiele, die aufs
  Hauptfeld gehören.

  **Freigehalten wird erst ab Spielbereitschaft**, nicht ab dem Setzen: Solange
  die Spieler noch im Halbfinale stehen oder in ihrer Pflichtpause sind, darf
  ein anderes Spiel auf das Feld. So steht das Hauptfeld nicht stundenlang
  leer.

  In der Liste steht das gewählte Feld als Marke (⌖) — sie färbt sich, wenn das
  Feld gerade belegt ist. Damit sieht man auch, **worauf** ein wartendes Spiel
  wartet.

  Von Hand darfst du das Spiel weiterhin überall hinlegen; es kommt nur eine
  Rückfrage. Ein Wunschfeld auf ein **gesperrtes** Feld lehnt der Turnier-PC
  ab — die Reservierung ersetzt die Sperre für diesen Zweck. Der Wunsch
  verschwindet von selbst, sobald das Spiel auf dem Feld steht, und gilt nur
  für das laufende Turnier.

  **Eine Einschränkung:** Für Spiele mit Wunschfeld gibt es keine
  Startzeit-Prognose. Die Vorhersage rechnet mit Hallen, nicht mit einzelnen
  Feldern — sie wäre systematisch zu früh und würde auch die Zeiten der
  anderen Spiele verschieben.
- **Warnung „Ergebnis fehlt"** (seit v0.9.259): Ist ein Spiel nach seinen
  Sätzen entschieden — die Endpunkte sind erreicht —, das Feld aber weiterhin
  belegt, meldet die Oberfläche das nach **einer Minute**: als rote Marke an
  der Feld-Kachel und als Zeile im Störungsband oben. Meist heißt das, dass
  das Ergebnis nicht übermittelt wurde.

  Die Zeile im Band nennt Feld und Halle und **ignoriert den Hallenfilter** —
  sonst verschwiege sie genau den Fall, für den sie gedacht ist. Bei mehreren
  Feldern steht der älteste Fall zuerst. Verliert das Gerät die Verbindung,
  hat die Verbindungsmeldung Vorrang: Ohne Verbindung ist jede andere Aussage
  wertlos.

  **Wann sie bewusst schweigt:** in der Satzpause, während ein Satz läuft, bei
  Aufgabe/Kampflos/Disqualifikation, auf freien Feldern — und vor allem dann,
  wenn das Ergebnis bereits beim Turnier-PC liegt und nur auf BTP wartet.
  Dorthin muss niemand laufen. Ist die Zählweise eines Spiels unbekannt, wird
  ebenfalls nicht gewarnt.

  **Abschalten:** In der `config.json` steht `finished_warning_seconds`
  (Default 60). `0` schaltet die Warnung ganz ab — der Rückzugsweg, falls sie
  im laufenden Turnier zu oft danebenliegt.
- **Vom Feld nehmen im ⋯-Menü** (seit v0.9.280): Ein belegtes Feld bietet in
  seinem ⋯-Menü **„↩ Vom Feld nehmen"** — derselbe Befehl wie im Band nach dem
  Antippen der Kachel, nur ohne vorherige Auswahl. Läuft dort schon ein
  Spielstand, fragt die Seite nach, weil er verloren geht.
- **Felder sperren und freigeben** (seit v0.9.258): Im ⋯-Menü einer
  Feld-Kachel steht **„🔒 Feld sperren"**. Ein gesperrtes Feld bekommt von der
  automatischen Vergabe kein Spiel mehr — für ein kaputtes Netz, ein
  reserviertes Feld oder eine Halle, die gegen Abend verkleinert wird. Es ist
  dieselbe Sperre wie am Turnier-PC, nicht eine zweite.

  Läuft gerade ein Spiel darauf, heißt der Eintrag **„Feld nach diesem Spiel
  sperren"** — genau so verhält es sich auch: Das laufende Spiel zählt
  ungestört zu Ende, erst danach bleibt das Feld leer. Wer es sofort räumen
  will, nimmt das Spiel zusätzlich von Hand herunter.

  **„🔓 Feld freigeben"** fragt nach, bevor es wirkt. Das ist Absicht: Sperren
  kann man gefahrlos zurücknehmen, aber ein versehentlich freigegebenes Feld
  bekommt sofort wieder ein Spiel — und wer die Sperre nicht selbst gesetzt
  hat, sieht ihr nicht an, ob dort etwas kaputt ist.

  Zwei Dinge noch: Solange kein Turnier geladen ist, gibt es nichts zu sperren
  (die Felder kommen ja aus BTP). Und die Sperren gelten für **dieses**
  Turnier — am nächsten Tag mit einem anderen Turnier sind sie weg, weil BTP
  die Feldnummern neu vergibt und eine alte Sperre sonst ein beliebiges
  anderes Feld träfe.
- **Ein pausierendes Spiel lässt sich trotzdem aufs Feld legen** (seit
  v0.9.253): Die Mindestpause ist eine Empfehlung, keine Sperre — der
  Turnier-PC hat sie noch nie abgelehnt. Die Zeile bleibt deshalb wählbar
  und ziehbar; vor dem Schreiben kommt eine **Rückfrage**, die sagt, wer
  noch bis wann pausiert. Wer gerade auf einem **anderen Feld spielt**,
  bleibt dagegen unwählbar — das lehnt der Turnier-PC ab.
- **Jeder Name ist ein Link** auf die Spielerseite von badhub
  (`badhub.de/spieler/<Nr>/live`), erkennbar an der gepunkteten
  Unterstreichung; er öffnet in einem neuen Tab. Spieler ohne
  Lizenznummer in BTP bleiben unverlinkt. Seit 18.08.2026 gilt das an
  **allen drei Stellen**: in der Spielliste, in den **Feld-Kacheln** der
  laufenden Spiele und in der Liste der **beendeten Spiele**. Ein Klick
  auf den Namen einer Feld-Kachel öffnet nur die Spielerseite — er nimmt
  das Spiel **nicht** zusätzlich auf; ein vom Namen aus begonnener **Zug**
  auf ein anderes Feld funktioniert weiterhin.
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
  ggf. gekürzt. Jede Zeile hat seit v0.9.246 ein **⋮-Menü** mit
  „Ergebnis korrigieren", „Punktverlauf" und „Zettel" — dieselbe
  Zusammenstellung wie im ⋮-Menü der Feldkachel.
- **Ballzahl (seit v0.9.400):** Ist im Setup **Bälle am Tablet zählen**
  eingeschaltet, zeigt die Feldkachel neben dem Satzstand die am Tablet
  gezählten Bälle (Federball-Symbol mit Zahl) und die Beendet-Zeile die Zahl aus BTP. Bei 0 steht
  nichts. Der Dialog „Ergebnis eintragen" bzw. „Ergebnis korrigieren" hat dann
  ein Feld **Bälle** (0–99): beim Eintragen mit der Tablet-Zahl vorbelegt, beim
  Korrigieren mit dem Wert aus BTP. Ein leeres Feld heißt „keine Angabe" — in
  BTP bleibt dann, was dort steht. Eine Korrektur, die nur die Ballzahl
  ändert, ist erlaubt; es gelten dieselben Sperren wie für jede Korrektur. Die
  Zahl am laufenden Spiel ist ein Zwischenstand vom Tablet und fehlt, solange
  das Tablet keinen Stand gespiegelt hat. Mehr: [ballzaehler.md](ballzaehler.md).
  <!-- pruef: "leer = keine Angabe" in src-tauri/assets/tl.html -->
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
Spielpunkt einen zweiten, umlaufenden Rahmen plus Abzeichen: **„Satzpunkt"**
für den letzten Punkt eines Satzes, **„Spielpunkt"** (pulsierend — das Feld
wird gleich frei) für den letzten Punkt des Matches. Das ist nur eine
Planungshilfe für die Turnierleitung, keine Wertungslogik, und erscheint
**ausschließlich hier**, nie auf den Hallen-TVs (Court-Monitor/Overview) —
bewusste Scope-Entscheidung vom 20.07.2026, Plan 16.

#### Abzeichen: drei Dringlichkeitsstufen

Alle kleinen Abzeichen an Feldkacheln und Listenzeilen folgen **einer**
Skala — man muss sich nicht je Abzeichen merken, was seine Farbe bedeutet:

| Stufe | Sieht so aus | Was da steht |
|---|---|---|
| **Info** | ruhig, nur umrandet | „Satzpunkt", „Spielpunkt", „manuell einsortiert" — Planungshilfe, nichts zu tun |
| **Warnung** | gelb-orange gefüllt | „⏸ Auto-Vergabe aus", Schiedsrichter-Konflikt — bewusst gesetzt oder nachsehen |
| **Alarm** | rot gefüllt | überfällig, gesperrt, Verletzung, „TL gerufen" — da muss jemand hin |

Info-Abzeichen sind bewusst nur **umrandet** statt ausgefüllt: „Spielpunkt"
ist dadurch auf den ersten Blick von einem Alarm zu unterscheiden, auch
wenn beide rötlich sind. Am Feld-**Streifen** heißt Rot weiterhin
„überfällig", nie „Spielpunkt".

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

### Spiele, deren Paarung noch nicht feststeht (seit v0.9.273)

In der Spielliste stehen auch Spiele, bei denen noch gar nicht klar ist, wer
gegeneinander antritt — ein Halbfinale, dessen Vorrunde noch läuft. Sie
erscheinen an ihrer regulären Stelle der Reihenfolge, ruhiger gefärbt als die
übrigen Zeilen und mit der Marke **⋯ noch offen**.

Was in der Zeile steht, hängt davon ab, wie viel BTP hergibt:

| Anzeige | Bedeutung |
|---|---|
| `Müller oder Schmidt` | Die beiden möglichen Teilnehmer aus dem direkten Vorspiel. |
| `aus Spiel 42` | Das Vorspiel ist bekannt, seine Teilnehmer aber selbst noch nicht. Bewusst **nicht** „Sieger aus 42": Bei Platzierungsspielen füllt der Verlierer den Platz, und welche Seite es ist, sagt BTP nicht. |
| `noch offen` | Es gibt kein auffindbares Vorspiel — ein Setzplatz, ein Freilos oder ein Qualifikant aus einer Gruppe. |

Steht **eine** Seite schon fest, zeigt sie ihre echten Namen und nur die
andere einen dieser Texte.

**Was an so einer Zeile geht:** umsortieren (Griff, „An den Anfang" im ⋮-Menü), Spielort setzen,
Wunschfeld, Auto-Vergabe-Ausnahme, Schiedsrichter einteilen (seit v0.9.355). Eine manuell gesetzte Position bleibt
erhalten, wenn die Paarung später feststeht — das Spiel springt nicht zurück.

**Was nicht geht:** in die Vorbereitung rufen, einem Feld zuweisen, Ergebnis
eintragen, Vorabzettel drucken. Es gibt schlicht niemanden aufzurufen oder
aufzuschreiben; die Knöpfe fehlen deshalb ganz, statt eine Fehlermeldung zu
bringen. Die automatische Feldvergabe überspringt diese Spiele ebenfalls, und
sie bekommen **keine** Startzeit-Prognose — sonst verschöben sie die Prognose
aller anderen.

**Setzplätze** (seit v0.9.298): Gesetzte Paarungen tragen in der Spielliste
und auf der Feldkachel ihren Setzplatz hinter dem Namen, z. B. `[1]` oder
`[3/4]`. Das hilft bei der Frage, welches Spiel aufs Hauptfeld gehört. Im
**Profile**-Knopf lässt sich das mit dem Häkchen „Setzplätze zeigen"
ausschalten; standardmäßig ist es **an**, auch in bestehenden Profilen.
Spiele mit noch offener Paarung und beendete Spiele zeigen keine Setzung.
<!-- pruef: "Setzplätze zeigen" in src-tauri/assets/tl.html -->

**Ausblenden:** Wer lieber die kurze Arbeitsliste sieht, nimmt im
**Profile**-Knopf das Häkchen „Spiele mit noch offener Paarung zeigen"
heraus. Der Schalter gilt für das Profil, also je Gerät — standardmäßig ist
er **an**, auch an Geräten, die es schon vor diesem Update gab.

> **Große Turniere über die Cloud:** Reicht der Platz im Zustand nicht für
> alles, lässt der Turnier-PC zuerst die offenen Spiele weg und erst danach
> etwas anderes. Die Arbeitsliste wird durch dieses Feature also nie kürzer.
> Wie viele Vorschau-Zeilen dabei fehlen, steht unter der Liste („… weitere
> Spiele mit noch offener Paarung sind nicht mit übertragen").
> Im Hallennetz (LAN) tritt das praktisch nicht auf.

### Spiele in der Warteliste umsortieren

**Der schnellste Weg — „ganz nach vorn":** im **⋮-Menü** der Zeile der
zweite Eintrag, **↑ An den Anfang der Spielliste**. Ein Tipp, und das Spiel
steht an der Spitze der umsortierbaren Liste; das Menü schließt dabei von
selbst. Kein Ziehen, kein Scrollen, egal wie lang die Liste ist. Der
Eintrag fehlt an einem Spiel, das schon vorn steht oder bereits gerufen ist
— dort täte er nichts.

> Von v0.9.271 bis v0.9.323 stand dafür ein eigener ↑-Knopf an der Zeile.
> Seit v0.9.324 ist er wieder im Menü — diesmal ganz oben und hier
> beschrieben, und weil das Menü nach der Wahl zugeht, kostet „ganz nach
> vorn" keinen Tipp mehr als vorher (Wunsch 06.09.2026: weniger Knöpfe je
> Zeile).

Am **Griff** (⠿) links an einer Wartelisten-Zeile lässt sich das Spiel nach
oben oder unten ziehen — auf dem Tablet wie mit der Maus. Ab dem ersten Zug
gilt für dieses Spiel deine Reihenfolge statt der von BTP; die Zeile bekommt
dafür die Marke „manuell einsortiert". Der Knopf im Kopf des Panels
„Spiele" nimmt die Handsortierung des ganzen Turniers wieder zurück.

**Hin und wieder zurück geht jetzt auch.** Ein Spiel vor ein anderes zu
setzen, es dann woandershin zu schieben und kurz darauf wieder an die erste
Stelle — das wirkte bisher beim dritten Mal nicht, wenn alles innerhalb von
zwanzig Sekunden passierte. Der Turnier-PC hielt den Zug für ein Versehen und
bestätigte ihn, ohne etwas zu tun. Jetzt zählt jeder Zug.

### Gerufene Spiele umsortieren

Gerufene Spiele stehen oben in der Liste und kommen **in der Reihenfolge
ihres Aufrufs** aufs Feld: Wer zuerst gerufen wurde, ist zuerst dran (seit
v0.9.347). Diese Reihenfolge lässt sich ändern:

- am **Griff** (⠿) einer gerufenen Zeile ziehen. Wer auf eine wartende
  Zeile ablegt, schiebt das Spiel ans Ende der Gerufenen;
- im **⋮-Menü** der Zeile: **⤒ An den Anfang der Gerufenen**,
  **↑ Eine Position nach oben**, **↓ Eine Position nach unten**;
<!-- pruef: "⤒ An den Anfang der Gerufenen" in src-tauri/assets/tl.html -->
- mit eingeschalteten Verschiebe-Knöpfen (Profil) auch direkt an der Zeile.

Im Mehr-Hallen-Turnier gilt die Reihenfolge je Halle: Verschoben wird nur
zwischen Spielen, die in dieselbe Halle gerufen wurden. Ein Zug über die
Hallengrenze wird mit einer Meldung abgelehnt. Wird ein Spiel in der
Warteliste nach oben gezogen, ändert das nichts an den Gerufenen. Es wird
nur als Nächstes gerufen und stellt sich dann hinten an. Mehr:
[Reihenfolge der Gerufenen](preparation.md#reihenfolge-der-gerufenen).

**Über lange Listen ziehen** (seit v0.9.271): Zieh die Zeile an den oberen
oder unteren Rand der Liste und **bleib dort stehen** — die Liste scrollt
weiter, solange du hältst. Wie schnell, hängt davon ab, wie weit du zum Rand
gehst: knapp in den Randbereich hinein kriecht sie (damit du genau treffen
kannst), ganz am Rand läufst du in gut zwei Sekunden durch die ganze Liste.
Über den Rand **hinaus** zu fahren ist ausdrücklich erlaubt und heißt
einfach „noch weiter". Auch sehr lange Wartelisten laden dabei nach; du
kommst also bis ans echte Ende, nicht nur bis dahin, wo die Liste beim
Anfassen aufhörte.

**Abbrechen:** Weit **seitlich** aus dem Panel fahren und dort loslassen —
die Zeile springt an ihren Platz zurück, und es wird nichts an den Turnier-PC
geschickt. Solange du seitlich draußen bist, ruht der Zug: Die Liste scrollt
nicht weiter, und die Reihenfolge ändert sich nicht. Das ist zugleich der
Schutz davor, dass ein Zug hinüber zu den Feldkacheln versehentlich umsortiert.

**Ein Tipp auf den Griff bleibt ein Tipp.** Erst ab ein paar Pixeln Bewegung
wird daraus ein Zug — den Griff nur zu berühren oder gedrückt zu halten ändert
nichts, auch nicht am Rand der Liste.

**Ohne Ziehen:** Den Griff antippen bzw. mit Tab anfokussieren und mit ↑/↓
verschieben. Ein Schritt je Tastendruck. Auch das **⋮-Menü** samt seinen
Einträgen und die übrigen Knöpfe der Zeile lassen sich mit Tab ansteuern und
mit Enter auslösen.

**Vier Verschiebe-Knöpfe statt Ziehen** (seit v0.9.280, Profil-Schalter):
Wo Ziehen nicht praktikabel ist (kleiner Bildschirm, ungenauer Stift,
Handschuhe), schaltet das Häkchen **„Verschiebe-Knöpfe an jeder Zeile"** im
Profil-Editor an jeder umsortierbaren Zeile vier Knöpfe frei: **⤒** an den
Anfang, **↑** eine Position nach oben, **↓** eine nach unten, **⤓** ans Ende.
Sie stehen direkt an der Zeile, vor dem ⋮-Menü (dessen Eintrag „An den
Anfang der Spielliste" bleibt trotzdem da), und immer in derselben Reihe; was
gerade nicht geht, ist ausgegraut statt versteckt: ⤒/↑ an der ersten Zeile,
↓/⤓ an der letzten — Letzteres nur, wenn die ganze Liste geladen ist. „Ans
Ende" heißt das echte Ende der Liste, auch wenn sie noch nicht bis dorthin
geladen ist; alles bis dahin Sichtbare gilt danach als von Hand sortiert.
„Eine nach unten" an der letzten geladenen Zeile schiebt vor das nächste
noch nicht geladene Spiel. Standard ist **aus** — die Zeile wird damit
spürbar breiter.

**Behoben in v0.9.280:**

- Das **Ziehen** am Griff endete seit v0.9.271 fast immer damit, dass die
  Zeile an ihren Platz zurücksprang und nichts gesendet wurde. Ursache: Der
  Browser gibt den Zeigerfang eines Elements frei, sobald es beim
  Einsortieren im DOM umgehängt wird, und das Sicherheitsnetz gegen
  verlorene Gesten hielt das für einen Abbruch. Jetzt holt sich der Zug den
  Fang zurück; nur wenn das nicht mehr geht, gilt die Geste als verworfen.
- **Zweimal „an den Anfang"** (erst Spiel A, dann Spiel B) warf A wieder auf
  seinen BTP-Platz zurück, statt es an zweiter Stelle zu halten. Der manuell
  sortierte Block bleibt jetzt bei jedem Zug vollständig erhalten.

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

Die Seite besteht aus bis zu acht **Panels** — Felder, Aufgaben
(Walkover), Zähltafel-Warteschlange, Schiedsrichter, Spiele, „Beendete
Spiele", „Spielzeiten" (nur mit Prognose) und „Anfangszeiten" (nur mit
eingerichtetem Check-In). Neue Panels erscheinen in Bestandsprofilen
automatisch in der letzten Spalte.
Jedes trägt dieselbe Kopfzeile: Titel, Anzahl, ein **Auge** zum Aus- und
Einblenden und einen **Pfeil** zum Zu- und Aufklappen.

- **Ausblenden ist dauerhaft** (nicht nur zugeklappt): Ein ausgeblendetes
  Panel belegt keinen Platz mehr und bleibt nach einem Neuladen der Seite
  ausgeblendet. Wer keine Schiedsrichter einteilt, wird den Abschnitt
  dadurch wirklich los.
- **Reihenfolge ändern**: Am Kopf jedes Panels sitzt ein Verschiebe-Griff
  (drei Balken — bewusst ein anderes Symbol als der ⠿-Griff, mit dem man
  einzelne *Zeilen* zieht). Ziehen oder mit den Pfeiltasten verschieben.
- **Höhe verteilen**: Zwischen je zwei sichtbaren Panels EINER Spalte
  sitzt ein Trennsteg. Ziehen gibt dem einen Panel mehr Platz und dem
  anderen weniger; **Doppeltipp** stellt die gleichmäßige Verteilung
  wieder her. Kein Panel lässt sich auf null ziehen. Ist ein Panel
  dazwischen ausgeblendet, greift der Steg automatisch das nächste
  sichtbare.
  **Die Verteilung hängt nur am gezogenen Verhältnis, nicht am Inhalt**
  (seit v0.9.229): Wie viele Zeilen ein Panel gerade zeigt, ändert seine
  Höhe nicht. Vorher wuchs die Spielliste beim Nachladen auf Kosten ihrer
  Nachbarn, und der Steg ließ sich danach nicht mehr sinnvoll ziehen.
  Umgekehrt heißt das: Ein Panel mit wenig Inhalt behält seinen Anteil,
  auch wenn darunter Platz frei bleibt — wer das anders will, zieht den
  Steg (das wirkt jetzt wieder zuverlässig). Das Panel **„Felder"** bildet
  die Ausnahme: Es gibt ungenutzte Höhe automatisch an die übrigen Panels
  seiner Spalte zurück, solange du seine Höhe nicht selbst gezogen hast.
  Ziehst du, tritt die Automatik zurück und dein Maß gilt; **Doppeltipp**
  auf den Steg schaltet sie wieder ein.

#### Spalten (1 bis 3)

Wie viele Spalten die Seite nebeneinander zeigt, steht im Profil — **eine,
zwei oder drei**. Jedes Panel trägt seine Spaltennummer; automatisch
umgebrochen wird nichts.

- **Voreinstellung: zwei Spalten**, Felder links, alles Übrige rechts —
  genau das Bild, das die Seite vorher als festes Zweiergespann hatte.
- **„Felder" ist ein Panel wie jedes andere** und darf in jeder Spalte
  stehen. Die frühere feste Ankerposition oben/links gibt es nicht mehr.
- **Spaltenbreite ziehen**: Zwischen zwei Spalten sitzt derselbe Steg wie
  zwischen zwei Panels, nur senkrecht. Ziehen verschiebt die Grenze,
  **Doppeltipp** verteilt wieder gleichmäßig. Eine Spalte lässt sich nicht
  beliebig schmal ziehen.
- **Panel in eine andere Spalte ziehen**: Am Verschiebe-Griff über die
  Spaltengrenze ziehen — das ändert Reihenfolge und Spalte in einem Zug.
  Eine noch leere Spalte lässt sich so nicht befüllen (sie ist gar nicht
  da); dafür gibt es die Spaltenauswahl je Panel im Profil-Editor.
- **Eine Spalte ohne sichtbares Panel** hinterlässt keinen leeren Streifen
  — die übrigen teilen sich den Platz.
- **Auf schmalen Geräten stehen die Spalten immer untereinander**
  (Tablet-Hochformat, Telefon), egal was das Profil sagt: Ein
  Wandmonitor-Profil darf ein Tablet nicht unbenutzbar machen.

Die frühere Einstellung „Spielliste rechts/darunter" geht in den Spalten
auf: „rechts" ist das Zwei-Spalten-Preset, „darunter" das mit einer
Spalte. Ein Profil aus einer älteren Version wird beim ersten Öffnen
entsprechend übersetzt und sieht dadurch unverändert aus.

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
- **Schriftgröße — nur dieses Gerät** (seit 17.08.2026): unten im selben
  Overlay, vier Stufen 85/100/115/130 %. Bewusst **kein** Teil des
  Profils — wie groß die Schrift sein muss, hängt am Display und
  Betrachtungsabstand genau dieses Geräts. Die Wahl hängt am Gerät
  (`localStorage`) und übersteht ein Neuladen; die Feldkacheln passen
  ihre Kompaktstufe automatisch an.

Der Profil-Editor ist in Gruppen geordnet (seit v0.9.344):

| Gruppe | Inhalt |
|---|---|
| **Aufbau der Seite** | Spaltenzahl, Panels (sichtbar, Reihenfolge, Spalte) |
| **Spielangaben** | Spielnummer, Disziplin/Klasse, Runde, Gruppe, Setzplätze |
| **Spieler** | Nationen, Vereinsnamen, Vereinslogos, Zuletzt gespielt |
| **Spielliste** | Spiele mit offener Paarung, Verschiebe-Knöpfe |
| **Felder** | Restzeit laufender Spiele, Aufrufe unbegrenzt |
| **Spielzeiten** | Gruppierung des Panels „Spielzeiten" |
<!-- pruef: "Aufbau der Seite" in src-tauri/assets/tl.html -->
<!-- pruef: "Spielangaben" in src-tauri/assets/tl.html -->

Häkchen, die der Turnier-PC noch nicht kennt (ältere Version), fehlen; eine
Gruppe ohne Häkchen erscheint dann gar nicht.

Die Anzeige-Häkchen im Profil-Editor:

- **Spielnummer zeigen** (Standard: an) — die Zahl ganz links in der Liste.
- **Nationen zeigen** (Standard: **aus**) — die **Flagge** neben jedem
  Namen, dieselben Bilder wie auf dem Court-Monitor. Zu einer Nation ohne
  Flaggendatei erscheint das Kürzel. Hilfreich bei internationalen
  Turnieren.
- **Disziplin/Klasse, Runde, Gruppe zeigen** (Standard: alle drei an) —
  einzeln abschaltbar in der Meta-Zeile der Warteliste; Feldkacheln und
  „Beendet" zeigen sie unverändert weiter.
- **Restzeit laufender Spiele zeigen** (Standard: **aus**) — die geschätzte
  Restzeit des laufenden Spiels („~12 min Rest") in der Fußzeile jeder
  belegten Feldkachel, zwischen Satzstand und Aufruf-Uhr. Die Schätzung
  rechnet der Turnier-PC aus Live-Satzstand und gemessenen Spielzeiten
  (siehe [spielzeiten-prognose.md](spielzeiten-prognose.md), Etappe D);
  ohne eingeschaltete Prognose erscheint nichts. Erfordert einen Host ab
  v0.9.212 — ältere Hosts senden das Feld nicht, die Kachel bleibt dann
  unverändert.
- **Zuletzt gespielt zeigen** (Standard: **aus**) — an jeder Seite einer
  wartenden oder gerufenen Paarung eine kleine graue Marke **„vor 42 min"**:
  wie lange das letzte Spiel dieser Seite her ist. Im Doppel zählt der
  Partner, der zuletzt gespielt hat (die kürzere Pause). Die Marke steht
  unabhängig von der Pflichtpause — läuft die noch, stehen Eieruhr und Marke
  nebeneinander. Kampflose Spiele (Walkover) zählen nicht, Aufgabe und
  Disqualifikation schon. Keine Marke erscheint, wenn die Seite heute noch
  nicht gespielt hat (ein Spiel, das länger als zwölf Stunden her ist, zählt
  nicht — etwa vom Vortag) oder einer ihrer Spieler gerade auf einem anderen Feld
  steht (dann ist sein Name rot). Der Tooltip nennt die Uhrzeit des
  Spielendes. Als Spielende gilt das Eintragen des Ergebnisses am Tablet
  oder in der Turnierleitung, sonst der Moment, in dem bts-light das
  Ergebnis in BTP sieht. Wird bts-light erst mitten im Turniertag
  eingerichtet, gelten alle bis dahin beendeten Spiele als „gerade eben"
  beendet. Erfordert einen Host ab v0.9.342; ältere Hosts zeigen das
  Häkchen nicht.
  <!-- pruef: "Zuletzt gespielt zeigen" in src-tauri/assets/tl.html -->

Daneben, nicht Teil der Profile, aber am selben Kopfbereich: **Automatik**
an- und abschalten — der Schalter oben rechts. Er zeigt **„Automatik läuft"**
oder **„Automatik pausiert"** und erscheint nur, wenn die automatische
Feldvergabe in den Einstellungen eingeschaltet ist.
<!-- pruef: "Automatik pausiert" in src-tauri/assets/tl.html -->

Die Pause **bleibt bestehen** (seit v0.9.323), auch wenn bts-light neu
gestartet oder aktualisiert wird oder jemand am Turnier-PC die Einstellungen
speichert. Sie gilt nur für das Turnier, in dem sie gesetzt wurde: Meldet BTP
ein anderes Turnier, läuft die Automatik wieder. Am Turnier-PC steht während
der Pause oben ein Balken **„Automatik pausiert"**; mit dem Knopf
**Fortsetzen** hebst du sie dort auf (seit v0.9.326) — praktisch, wenn das
Gerät, das pausiert hat, nicht mehr da ist.
<!-- pruef: "Automatik pausiert" in src/components/AutoAssignPauseBanner.tsx -->
<!-- pruef: "Fortsetzen" in src/components/AutoAssignPauseBanner.tsx -->

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
am Anzeigegerät). Im Cloud-Modus lädt die Seite die Logos direkt über den
öffentlichen badhub-Logo-Resolver (`/api/v1/club-logo`). **Beide Wege fragen
denselben Endpoint ab** — die Zuordnung des BTP-Vereinsnamens (inklusive
gängiger Abkürzungen wie „BC" für „Badminton Club" und Klammerzusätzen wie
„(Berlin)") macht badhub selbst, LAN und Cloud verhalten sich also gleich
(Befund 15.08.2026: eine frühere, bts-light-eigene Zuordnung im LAN-Modus war
schwächer als badhubs eigener Abgleich und ließ manche Vereine ohne Logo
stehen, obwohl badhub eines hatte). Fehlt ein Logo bei badhub tatsächlich,
bleibt es beim Namen. Der **Verein** ist wie die Nationalität ein bewusst
zuschaltbares, standardmäßig ausgeschaltetes Anzeige-Feld (Datenschutz).

### Spieler und Schiedsrichter suchen (seit v0.9.368)

Oben in der Kopfleiste steht ein Suchfeld **„Spieler oder SR suchen"**. Ab
zwei Buchstaben erscheint nach einer kurzen Tipp-Pause unter der Kopfleiste
eine Trefferliste mit allen Spielen dieser Person, in drei Gruppen:
**Am Feld**, **Warteliste** und **Beendet**.
<!-- pruef: "Spieler oder SR suchen" in src-tauri/assets/tl.html -->
<!-- pruef: "Am Feld" in src-tauri/assets/tl.html -->
<!-- pruef: /TL_SEARCH_MIN_LEN: usize = 2;/ in relay-proto/src/lib.rs -->

- **Gesucht wird übers ganze Turnier** — anders als die Panels, die nur die
  vorderen Wartespiele und die letzten 30 beendeten Spiele zeigen, findet die
  Suche auch ein Spiel vom Vormittag oder vom Vortag. Der Turnier-PC sucht
  selbst; die Seite fragt ihn nur.
  <!-- pruef: /const FINISHED_LIMIT: usize = 30;/ in src-tauri/src/tablet/tl.rs -->
- **Schreibweise ist egal**: „mueller", „muller" und „Müller" finden dasselbe,
  ebenso „jose" und „José". Mehrere Wörter müssen im Namen **einer** Person
  stehen, die Reihenfolge ist egal — „max mül" findet Max Müller, „Müller
  Schmidt" findet **nicht** das Doppel Müller/Schmidt.
- **Schiedsrichter** (SR und AR) findet die Suche nur, wenn der
  Schiedsrichter-Betrieb eingeschaltet ist. Sonst steht oben „Schiedsrichter-
  Betrieb aus". Ein kampflos gewertetes Spiel zählt nicht als Einsatz.
  <!-- pruef: "Schiedsrichter-Betrieb aus" in src-tauri/assets/tl.html -->
- Jeder Treffer zeigt Spielnummer, Klasse, Runde und die **Rolle** der
  gesuchten Person (Spieler, SR, AR oder „möglicher Teilnehmer" bei einem
  Spiel, dessen Paarung noch aus dem Vorspiel kommt). Wartende Spiele tragen
  die erwartete Startzeit („ca. 14:20") oder, weiter hinten in der Liste,
  „Platz N in der Warteliste". Beendete Spiele zeigen das Ergebnis — eine
  Uhrzeit nur, wenn bts-light das Spiel selbst laufen sah.
  <!-- pruef: "möglicher Teilnehmer" in src-tauri/assets/tl.html -->
- Je Gruppe stehen höchstens 50 Treffer da; darüber hinaus heißt es
  **„weitere N – bitte genauer suchen"**.
  <!-- pruef: /TL_SEARCH_HITS_PER_GROUP: usize = 50;/ in relay-proto/src/lib.rs -->
  <!-- pruef: "bitte genauer suchen" in src-tauri/assets/tl.html -->
- Die Liste ist ein **Standbild**: Oben steht „Stand hh:mm", der Knopf
  **„neu suchen"** holt den aktuellen Stand.
  <!-- pruef: "neu suchen" in src-tauri/assets/tl.html -->

**Ein Tipp auf einen Treffer** springt zur Zeile im Panel und lässt sie kurz
aufleuchten. Ist das Panel zugeklappt, klappt es auf (wie mit dem Pfeil — das
gilt für alle Geräte mit diesem Profil). Steht die Zeile gerade nicht in der
Liste, sagt der Treffer, warum:

- **„Liegt in Halle X – Filter umstellen?"** — der Hallenfilter zeigt eine
  andere Halle. Er wird erst mit dem Knopf umgestellt, nie von selbst.
- **„Der Filter ‚nur spielbereite' verbirgt dieses Spiel"** mit Knopf
  „Filter aus".
- **„nicht in der Liste sichtbar (Platz N)"** — das Spiel steht hinter dem
  Ende der Liste oder das Panel ist im Profil ausgeblendet. Bei beendeten
  Spielen lassen sich **Punktverlauf** und **Zettel** dann direkt am Treffer
  öffnen.
  <!-- pruef: "nicht in der Liste sichtbar" in src-tauri/assets/tl.html -->

**Mit der Tastatur:** Die Pfeiltasten **↑/↓** im Suchfeld wählen einen
Treffer (er bekommt einen farbigen Rand links), **Enter** löst ihn aus wie ein
Tipp. Ist keiner gewählt, springt **Enter** gleich zum **ersten** Treffer —
auch dann, wenn die Trefferliste noch gar nicht da war: Die Suche läuft sofort
los und springt, sobald die Antwort kommt. Der Cursor bleibt dabei im
Suchfeld, man kann also weitertippen. **Escape** im Suchfeld leert es und
schließt die Liste.
<!-- pruef: "ArrowDown" in src-tauri/assets/tl.html -->

Das Suchfeld lässt sich je Profil ausblenden (**Profile → Spielliste →
„Spielersuche zeigen"**). Das ist nur eine Anzeige-Einstellung, keine
Sperre. Der Suchbegriff wird nirgends gespeichert. Es erscheint erst, wenn
der Turnier-PC die Suche kennt (ab v0.9.368).
<!-- pruef: "Spielersuche zeigen" in src-tauri/assets/tl.html -->

Details: [features/tl-spielersuche.md](features/tl-spielersuche.md).

### Punktverlauf ansehen

Der **📈-Knopf** im ⋮-Menü einer belegten Feldkachel (seit 17.08.2026
dort statt auf der Kachel — eine Fläche weniger) und im ⋮-Menü der
Beendet-Zeilen öffnet den **Punktverlauf** des Spiels: je Satz ein
Liniendiagramm (x = Ballwechsel, y = Punkte, eine Linie je Partei).
Bei laufenden Spielen wächst die Kurve mit. Der Knopf erscheint nur,
wenn ein Tablet das Spiel gezählt hat — Papier-Ergebnisse haben keinen
Verlauf. Details: [punktverlauf.md](punktverlauf.md).

### Schiedsrichterzettel abrufen

Der **🖨-Knopf** im ⋮-Menü öffnet den **Schiedsrichterzettel** — sowohl an
einer belegten Feldkachel als auch (seit v0.9.246) an jeder Zeile der
Beendet-Liste, wo er am meisten Sinn ergibt: Das Spiel ist abgeschlossen,
der Zettel vollständig. Er erscheint als druckfertiges Dokument im Fenster,
„Drucken" schickt ihn an den Drucker des Geräts. Er steht unter derselben
Schranke wie der Punktverlauf: ohne Aufzeichnung kein Zettel. Details:
[features/schiedsrichterzettel-druck.md](features/schiedsrichterzettel-druck.md).

### Alle Tablets neu laden (⟳ Tablets, seit v0.9.268)

Oben rechts, neben **Profile**, liegt der Knopf **„⟳ Tablets"**. Nach einer
Rückfrage laden **alle Zähltablets** ihre Seite neu.

Gebraucht wird er selten: Ein Tablet merkt selbst, wenn seine Seite nicht mehr
die aktuelle ist, und holt sich den neuen Stand **zwischen zwei Spielen** von
allein. Der Knopf ist für den Fall, dass nicht darauf gewartet werden soll —
etwa nach einem Update in der Turnierpause.

**Er trifft auch Tablets mit laufendem Spiel.** Deren Bildschirm springt kurz.
Der Spielstand geht dabei nicht verloren — er liegt auf dem Gerät und beim
Turnier-PC und ist nach einem Augenblick wieder da. Trotzdem: besser zwischen
zwei Runden drücken als mitten im Ballwechsel.

**Nicht jedes Gerät antwortet darauf.** Ein Tablet, dessen geladene Seite noch
aus einer Zeit vor diesem Knopf stammt, kennt den Befehl nicht und verwirft
ihn stillschweigend. Solche Geräte holt nur der stille Abgleich ab — oder eine
Hand am Bildschirm. Erscheint der Knopf **gar nicht**, ist der Turnier-PC
älter als das Feature (siehe „Grenzen").

Und ein Gerät, das nichts speichern darf (Kiosk mit gesperrtem Browser-Speicher,
privater Modus), springt mitten im Spiel bewusst **nicht** — dort überlebte der
Stand das Neuladen nicht. Es zeigt stattdessen den Hinweis, und wer davorsteht,
entscheidet.

Details: [features/tablet-version-abgleich.md](features/tablet-version-abgleich.md).

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
- **In der Warteliste** (seit v0.9.348) eine Marke „SR: … · AR: …" an
  jedem Spiel, dessen Besetzung schon feststeht — in BTP vorab eingetragen
  oder hier von Hand zugewiesen. Warnfarbe bei einem Konflikt wie an der
  Kachel. Seit v0.9.354 warnt die Marke außerdem, wenn dieselbe Person an
  zwei wartenden Spielen eingeteilt ist (**„SR doppelt"** bzw. „AR doppelt")
  oder gerade pausiert (**„SR in Pause"** bzw. „AR in Pause"). Dienst auf
  einem laufenden Feld zählt nicht als doppelt. Die Einteilung bleibt in
  beiden Fällen bestehen; die Turnierleitung entscheidet. Seit v0.9.358
  zeigt die Marke eine Umteilung sofort, auch bevor BTP sie übernommen hat.
  Steht ein Spiel auf **Ohne Schiedsrichter**, bleibt das so, bis jemand von
  Hand oder in BTP wieder einteilt.
  <!-- pruef: "doppelt" in src-tauri/src/tablet/tl.rs -->
  <!-- pruef: "in Pause" in src-tauri/src/tablet/tl.rs -->
- **Einteilen in der Warteliste** (seit v0.9.355): Im **⋮-Menü** jeder
  Wartelisten-Zeile steht **Schiedsrichter einteilen** — auch an Spielen,
  deren Paarung noch offen ist. Der Dialog heißt „Spiel N · Paarung" (an
  einer offenen Paarung mit den möglichen Teilnehmern) und zeigt die
  aktuelle Besetzung vorausgewählt. Jede Person trägt einen Zusatz, wenn
  sie gerade nicht frei ist: **(im Dienst)** auf einem Feld,
  **(reserviert Sp. N)** für ein anderes wartendes Spiel (ohne Spielnummer
  nur „(reserviert)") oder **(Pause)**. Die Wahl gilt sofort, ein
  Speichern-Knopf ist nicht nötig. Danach steht die Person an der Zeile
  („SR: … · AR: …") und ist für dieses Spiel reserviert: Die Rotation setzt
  sie an keinem anderen Feld ein.
  <!-- pruef: "Schiedsrichter einteilen" in src-tauri/assets/tl.html -->
  <!-- pruef: "reserviert Sp. " in src-tauri/assets/tl.html -->
  Den Eintrag gibt es nur, wenn der Turnier-PC mindestens v0.9.354 hat; an
  einem älteren fehlt er, die Feld-Kachel geht weiter.
- **In der Schiedsrichter-Liste** (seit v0.9.355) trägt eine reservierte
  Person die Marke **„reserviert Sp. N"**. Die Vorschau „nächster: …"
  überspringt reservierte Personen, weil die Rotation sie nicht einsetzt.
- **Vorschau als Nächstes** (seit v0.9.364): Der Turnier-PC rechnet aus,
  wen die Automatik als Nächstes einsetzen würde — nach Einteilungsprinzip
  und Rolle. Der Kopf der Liste zeigt dann **„nächster SR: … · AR: …"**, jede
  Zeile ihren voraussichtlichen Dienst („als Nächstes SR", sonst „→ AR").
  Personen mit fester Rolle tragen die Marke **„nur SR"** bzw. **„nur AR"**;
  im Einteilungsdialog steht der Hinweis schon vor der Wahl. In Modus
  „Getrennte Listen" zeigt die Seite die **SR-Reihenfolge**; die
  AR-Reihenfolge steht seit v0.9.365 darunter (siehe unten). Ein älterer Turnier-PC liefert
  keine Vorschau, dann bleibt es bei „nächster: …".
- **AR-Reihenfolge und Rolle im TL-Web** (seit v0.9.365): In Modus
  „Getrennte Listen" steht unter der SR-Reihenfolge eine zweite, eigene
  **AR-Reihenfolge**, ebenso ziehbar. Im Dialog einer Person (Klick auf die
  Einsatz-Zahl) lässt sich die **Rolle** wählen: „SR und AR", „nur
  Schiedsrichter" oder „nur Aufschlagrichter" — sie gilt sofort.
  <!-- pruef: "AR-Reihenfolge" in src-tauri/assets/tl.html -->
  <!-- pruef: "nur Aufschlagrichter" in src-tauri/assets/tl.html -->
  <!-- pruef: "als Nächstes " in src-tauri/assets/tl.html -->
  <!-- pruef: "SR-Reihenfolge" in src-tauri/assets/tl.html -->

Eine Zuweisung mit Konflikt wird **ausgeführt** und nur gekennzeichnet; die
Turnierleitung entscheidet. **Abgelehnt** wird sie dagegen (seit v0.9.354),
wenn das Spiel schon beendet ist (auch das Lösen) oder im Turnierstand fehlt
und wenn dieselbe Person Schiedsrichter **und** Aufschlagrichter desselben
Spiels wäre. Die Seite zeigt dann die Meldung des Turnier-PCs, gespeichert
wird nichts, und die Auswahl springt auf den gespeicherten Stand zurück.

**Tauschen** (seit v0.9.355, an Kachel und Warteliste): Wählt man als
Schiedsrichter die Person, die gerade Aufschlagrichter ist (oder umgekehrt),
tauscht der Dialog die beiden. Er löst dafür zuerst den anderen Dienst, teilt
dann ein und setzt die bisherige Person in den anderen Dienst. So entsteht
kein Zwischenstand, in dem eine Person beides wäre. War der eigene Dienst
vorher leer, bleibt der andere danach leer. Lehnt der Turnier-PC das
Einteilen ab, holt der Dialog den gelösten Dienst zurück. Scheitert erst der
letzte Schritt, sagt die Meldung, wer jetzt nicht mehr eingeteilt ist. Die
Auswahl zeigt danach den Stand, den der Turnier-PC bestätigt hat. Kurz nach
einer Änderung kann ein neu geöffneter Dialog noch den BTP-Stand zeigen, bis
BTP die Änderung übernommen hat.
<!-- pruef: "ist schon beendet — die Besetzung lässt sich nicht mehr ändern." in src-tauri/src/tablet/officials.rs -->
<!-- pruef: "zugleich Schiedsrichter und Aufschlagrichter sein." in src-tauri/src/tablet/officials.rs --> Steht in BTP schon ein Schiedsrichter am Spiel,
**ersetzt** die Auswahl hier ihn: Sie geht nach BTP, bis zur Übernahme zeigt
die Kachel kurz noch den alten Namen (seit v0.9.350). Ändert danach jemand
die Besetzung **in BTP**, gilt BTP — bts-light schreibt nicht dagegen.

Sperrlisten, Verein und Einsatz-Liste stehen **nicht** im Zustand, den alle
gekoppelten Geräte bekommen: Sie kodieren persönliche Beziehungen und werden
erst beim Öffnen der Pflege gezielt und mit dem Geräte-Zugang abgerufen
(`/tl/api/officials/{id}`, gleiches Muster wie der Punktverlauf).

**Im Cloud-Betrieb** funktioniert die Schiedsrichter-Bedienung erst, wenn der
Relay auf badhub.de die neuen Aktionen kennt (Deploy vor dem Client-Release);
im Hallennetz sofort.

### Pausen am Feld

Läuft auf einem Feld eine BWF-Pause (60 s bei 11 Punkten, 120 s Satzpause),
zeigt die Feldkachel **„Pause 1:23"** mit laufendem Countdown. Nach Ablauf
wechselt die Marke auf rotes **„überzogen +0:41"** und zählt hoch, bis am
Tablet weitergespielt wird — das Tablet hält die Pause seit v0.9.207
bewusst, statt sie automatisch zu beenden (ADR
[0028](adr/0028-pause-haelt-bis-weiterspielen.md)). Eine Behandlungspause
erscheint als **„Behandlung seit 3 min"** (ohne Countdown). Ein Tablet mit
älterer Seite beendet die Pause weiter automatisch bei 0 — dann verschwindet
die Marke einfach, „überzogen" erscheint nie fälschlich.

### Automatische Hallen-Vorverteilung (Mehr-Hallen)

> Spec: [features/hallen-vorverteilung.md](features/hallen-vorverteilung.md) ·
> ADR [0029](adr/0029-hallen-vorverteilung-eigener-store.md)/[0030](adr/0030-halle-bindet-die-feldvergabe.md)

Im Kopf des Panels **„Spiele"** (nur bei Mehr-Hallen-Turnieren): Der
Schalter **„Vorverteilung an (N)"** gibt den vordersten **x** Spielen der
Liste automatisch eine Halle — im Verhältnis der entsperrten Felder je
Halle und gemischt (bei 2:1 also A, A, B, A, A, B). Das x-Feld daneben
setzt die Fenstergröße (leer/0 = automatisch: Gesamtzahl der Felder).
Endet ein Spiel oder kommen neue hinzu, füllt die Automatik nach. Die
Zuordnung erscheint als Hallen-Kürzel mit **„·A"** an der Zeile, auf den
Hallen-Monitoren und im badhub-Aushang (`display=next&halle=…`) — Spieler
können also früh in ihre Halle gehen, das Feld folgt kurz vorher.

Regeln: Die Automatik füllt nur Spiele **ohne** Halle (Regel/Hand/Aufruf
bleiben und zählen aufs Verhältnis an); einmal verteilt heißt fest — nur
du änderst (Hallenwähler im ⋮-Menü ersetzt die Auto-Halle; ein
Vorbereitungs-Aufruf ebenso). **„Auto-Hallen räumen"** nimmt ausschließlich
die automatischen Zuordnungen zurück. Bei gesetzter **Tages-Halle** ist die
Vorverteilung ausgegraut — die beiden Modi schließen sich aus.

**Wichtig — Verhaltensänderung seit v0.9.208 (ADR 0030):** Eine gesetzte
Halle (Regel, Hand oder Auto) **bindet** jetzt die automatische
Feldvergabe: Das Spiel bekommt nur Felder seiner Halle, auch wenn woanders
eines frei ist. Und ein automatisch vorverteiltes Spiel wird **ohne
Vorbereitungs-Aufruf** vergeben — wer weiterhin Ansagen will, ruft wie
gewohnt (der Aufruf übernimmt dann die Halle).

### Hallen-Farben (Mehr-Hallen)

> Spec: [features/hallen-farben.md](features/hallen-farben.md) ·
> ADR [0031](adr/0031-hallen-farben-eigener-config-store.md)/[0032](adr/0032-hallen-farben-deterministische-auto-palette.md)/[0033](adr/0033-hallen-farben-hex-auf-dem-draht.md)

Bei Mehr-Hallen-Turnieren trägt jede Halle eine kleine **Farbmarke** neben
jeder Nennung: am Hallen-Filter, an den Hallen-Überschriften der
Feld-Gruppen, im Hallen-Kürzel der Wartelisten-Zeilen (auch bei „·A"-
Vorverteilung), an den Beendet-Zeilen und im Spielort-Wähler des ⋮-Menüs.
Die Farben vergibt der Turnier-PC automatisch (gleiche Farbe auf allen
Geräten, Monitoren und dem Aushang); ändern lässt sich die Farbe je Halle
auf der **Felderübersicht** der Desktop-App (Zahnrad am Gruppenkopf).
Kürzel und Name bleiben immer stehen — die Farbe ist eine Zusatz-Kennung,
nie die einzige Information. An einem alten Turnier-PC ohne das Feature
bleibt die Seite schlicht farblos.

### Startzeit-Prognose & Spielzeiten

**Gruppieren nach (seit v0.9.231).** Die Auswertung zeigt dieselben
Messwerte wahlweise nach **Konkurrenz** (Klasse × Disziplin, z. B. „HE-A" —
die Voreinstellung), nach **Klasse**, nach **Disziplin** oder nach **Halle**.
Die Wahl steht im Profil-Editor unter „Spielzeiten gruppieren nach" und gilt
damit für alle Geräte mit diesem Profil.

Die Hallen-Auswertung beantwortet die Frage, wegen der man bei zwei Hallen
überhaupt auf die Uhr schaut: Läuft eine Halle systematisch langsamer? Zwei
Dinge dazu:

- Bei **Ein-Hallen-Turnieren** wird die Hallen-Achse gar nicht erst zur
  Auswahl angeboten — dort stünde nur eine einzige Zeile. Trägt ein Profil
  sie bereits (von einem anderen Turnier), bleibt die Auswahl erhalten und
  das Panel sagt, dass es ersatzweise nach Konkurrenz gruppiert.
- Die Halle wird beim **ersten Aufruf aufs Feld** festgehalten. Spiele, die
  vor dem Update auf v0.9.231 gemessen wurden, kennen ihre Halle nicht mehr
  und stehen in einer eigenen Zeile **„ohne Halle"**. Wer mitten im Turnier
  aktualisiert, fängt die Hallen-Auswertung also praktisch neu an; die
  übrigen drei Achsen sind davon nicht betroffen.


> Details und Messregeln: [spielzeiten-prognose.md](spielzeiten-prognose.md) ·
> Spec [features/spielzeiten-prognose.md](features/spielzeiten-prognose.md)

Jedes wartende Spiel in der Liste trägt den **voraussichtlichen Aufruf**
(„🕐 14:32"), simuliert aus den gemessenen Spielzeiten dieses Turniers
(Median je Klasse × Disziplin), den Feldern, der Reihenfolge, Hallen-Regeln
und Spieler-Mindestpausen. **„~14:32"** heißt: noch keine Messwerte, es gilt
der Startwert aus dem SetupWizard („Startzeit-Prognose", Standard 25 min);
**„gleich"** heißt: als Nächstes dran. Ausgenommene Spiele bekommen keine
Prognose. Das Panel **„Spielzeiten"** zeigt die Auswertung: je Klasse und
Disziplin die Mediane von Brutto (Feldzuweisung → Ende), Netto (1. Punkt →
Ende) und Anlaufzeit, dazu die Zahl der Messungen; beendete Spiele tragen
ihre Ist-Zeiten in der Beendet-Zeile. Alles reine Anzeige — es geht nichts
davon an Monitore oder den Liveticker.

## Anfangszeiten (Check-In-Zeitplan, seit 17.08.2026)

Ist der **Hallen-Check-In** eingerichtet
([spieler-check-in.md](spieler-check-in.md)), gibt es das Panel
**„Anfangszeiten"**: der heutige Check-In-Zeitplan, wie ihn badhubs
öffentliche `…/zeitplan`-Seite zeigt — je Klasse **Anfangszeit**,
**Anmeldeschluss** (ohne eigenen gilt die Anfangszeit, wie bei der
Lautsprecher-Ansage) und der Stand **„eingecheckt/gemeldet"** (ohne
Abgemeldete, grün wenn alle da sind — dieselbe Rechnung wie die
Desktop-Check-In-Seite). Sortiert nach Anfangszeit; durchgelaufene
Schlüsse bleiben ausgegraut stehen, damit man sieht, was schon durch
ist. Bewusst **nur Zähler, keine Namen** — wer wissen will, *wer* fehlt,
nimmt die Check-In-Seite der Desktop-App. Der Turnier-PC fragt badhub
dafür höchstens minütlich, unabhängig vom Liveticker-Takt; kurze
Internet-Aussetzer leeren das Panel nicht — dauert der Aussetzer länger
als fünf Minuten, sagt eine ⚠-Zeile ehrlich, dass der Stand alt sein
kann.

## Aktualisierung: Push statt Dauerfragen (seit 18.08.2026)

Die Seite wird vom Turnier-PC **geweckt**, statt alle zwei Sekunden zu
fragen: Ein schmaler Kanal meldet „es gibt einen neuen Stand", die Seite
holt ihn daraufhin sofort über denselben Weg wie bisher. Sichtbar wird
das als **kürzere Reaktionszeit** (unter einer Sekunde statt bis zu
zwei) und als deutlich ruhigeres Netz — ein Tablet fragt im ruhigen
Betrieb nur noch alle 30 Sekunden nach, statt zweimal pro Sekunde.

Steht der Kanal nicht (älterer Turnier-PC, älterer Relay, Firmennetz
ohne WebSocket), fällt die Seite **geräuschlos** auf das bisherige
Verhalten zurück — alle zwei Sekunden fragen. Es gibt nichts
einzustellen und nichts zu bemerken; auch die Verbindungsanzeige oben
rechts bedeutet unverändert dasselbe.

Seit v0.9.287 hat der Stand-Abruf eine **Frist** (Kopfzeilen 5 s, Rumpf
15 s): Ein Abruf, der weder scheitert noch antwortet — etwa weil das Handy
zwischen zwei Access-Points gewechselt hat —, hielt vorher die Seite fest,
und die Verbindungsanzeige meldete dabei weiter „aktuell", weil nur ein
**gescheiterter** Abruf zählt. Jetzt endet er als Fehler, die Anzeige zeigt
das Alter der Daten, und der nächste Abruf läuft auf frischer Verbindung.
Dieselbe Regel wie bei den Court-Monitoren
([court-monitor.md](court-monitor.md), „Frist für den Stand-Abruf").

Technisch: `/tl-ws` trägt nur die Revisionsnummer, nie Turnierdaten
(Spec [features/tl-web-push.md](features/tl-web-push.md), ADR 0034).
Nebeneffekt am Turnier-PC: Er rechnet den Zustand jetzt **einmal
zentral** je Sekunde statt einmal je Gerät und Anfrage — bei acht
Geräten ist das der Unterschied zwischen acht Rechnungen pro Sekunde und
einer.

## Render-Sparsamkeit (seit 17.08.2026, erweitert 18.08.2026)

Nachtrag: Die **Beendet-Liste** wird jetzt je Spiel abgeglichen — ein
neues Ergebnis fügt eine Zeile ein, statt die Liste neu zu bauen (sie
selbst bleibt kurz: der Turnier-PC liefert höchstens 30 beendete
Spiele). Ihre Zeilen kosten außerhalb des sichtbaren Ausschnitts kein
Layout mehr (`content-visibility`). Spielzeilen und Feldkacheln bleiben
davon ausgenommen: Erstere tragen ihr fest positioniertes ⋮-Menü, das
sonst an der Zeilenkante abgeschnitten würde, und bei den Kacheln wird
die echte Größe gemessen.


Die Seite pollt weiter alle 2 s mit ETag-Kurzschluss, zeichnet bei einer
echten Änderung aber nicht mehr alles neu: Jedes Panel überspringt den
Neuaufbau, wenn sein Inhalt unverändert ist (`setzeHtmlWennNeu`), und die
zwei heißesten Listen — Feldkacheln und Spielzeilen — werden je
`court_id`/`match_id` **keyed abgeglichen** (`reconcileKeyed`): Ein
Punktgewinn ersetzt genau eine Kachel, alles andere bleibt samt offenem
⋯-Menü, Fokus und Scroll-Position stehen. Sekündliche Zeittexte stehen
deshalb nicht mehr im erzeugten HTML; `tickClocks()` füllt sie nach jedem
Render und weiter im Sekundentakt. Für Messungen:
`localStorage.tlRenderMessen = "1"` in der Browser-Konsole protokolliert
jede Render-Dauer.

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
- **Eine Halle ohne bedienbare Felder hält ihre Spiele fest.** Seit ADR
  0030 bindet eine gesetzte Halle (Regel, Hand oder Auto) die Feldvergabe.
  Automatische Zuordnungen räumt die App selbst, wenn ihre Halle keine
  Felder mehr hat — bei **Regel- oder Hand-Hallen** dagegen wartet das
  Spiel still, bis dort wieder ein Feld frei/entsperrt ist. Abhilfe von
  Hand: im Hallenwähler des ⋮-Menüs „–" wählen (Halle entfernen) oder eine
  andere Halle setzen.
- Über den **Cloud-Weg** ist die Warteliste turnierweit auf 40 Spiele
  gekürzt (im Hallennetz sind es 120; seit ADR 0026 gilt beide Grenzen
  hallenübergreifend, nicht mehr je Halle). Der Zustand geht bei jedem
  Ballwechsel neu über die Leitung; die volle Liste wäre ein Dauerstrom
  über Mobilfunk. Was fehlt, sagt die Seite.

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

## Schiedsrichterzettel drucken

Spec: [features/schiedsrichterzettel-druck.md](features/schiedsrichterzettel-druck.md) ·
ADR 0037/0038/0039.

Im Kebab-Menü einer belegten Feldkachel steht neben „📈 Punktverlauf" jetzt
**„🖨 Zettel"** — unter derselben Schranke: nur, wenn das Tablet wirklich etwas
aufgezeichnet hat. Ein Papier-Ergebnis bekommt bewusst keinen Zettel; ein halb
ausgefüllter Bogen wäre irreführender als keiner.

Der Zettel öffnet sich als Vorschau; **Drucken** öffnet den Druckdialog des
Browsers, „als PDF speichern" läuft über den Systemdialog des Druckers. Deshalb
braucht es weder eine PDF-Bibliothek noch eine zusätzliche Datei-Berechtigung.

**Anders als der Punktverlauf wird der Zettel nicht gepollt.** Er ist ein
Abschluss-Dokument, kein Livebild — und er trägt Sanktionsdaten (Karten), die
nicht im Zwei-Sekunden-Takt über die Leitung sollen. Beim Schließen wird er aus
dem DOM entfernt.

Der Abruf läuft über `GET /tl/api/scoresheet/{ids}` — **denselben Pfad am
eingebetteten Server wie am Relay**, deshalb ist die Seite in beiden
Betriebsarten identisch. Mehrere Kennungen mit Komma ergeben einen Stapeldruck
(Deckel: 40).

> **Der Relay hält keine Zettel vor.** Ist der Turnier-PC getrennt, gibt es 503
> und kein zwischengespeichertes Dokument — beim Punktverlauf ist das eine Frage
> des Mobilfunk-Budgets, hier zusätzlich eine des Datenschutzes.

## Meetingpoint (seit v0.9.330)

Ist der Meetingpoint am Turnier-PC eingeschaltet ([meetingpoint.md](meetingpoint.md)):

- Jede gerufene Partie trägt neben „gerufen" eine Marke `🧍 x/y` (eingecheckt
  von nötig) bzw. **✓ spielbereit**. Mit eingeschalteter Warnung wird die
  Marke nach der eingestellten Zeit orange und nennt die Fehlenden.
- Im ⋮-Menü der Zeile steht die Gruppe **Meetingpoint**: je Person ein Knopf
  zum Ein-/Auschecken von Hand (`meetingpoint_set`), bei SR-Pflicht dazu
  **Ohne Schiedsrichter** (setzt die vorhandene Aktion `official_clear`).
- Oben neben den Hallen schaltet **✓ nur spielbereite** die Spielliste auf
  eingecheckte Partien (je Gerät gemerkt).
- Im strikten Modus meldet das Störungsband, wenn ein Feld seit über einer
  Minute frei steht, ohne dass eine Partie bereit ist
  (`TlState.meetingpoint.idle_since_ms`).

Wire: `TlMatch.meetingpoint` (`relay_proto::TlMeetingpoint`, nur Flags und
Namen), `TlState.meetingpoint` (`TlMeetingpointLage`), Aktion
`TlAction::MeetingpointSet{matchId, platz, present}`. Der Relay parst
`TlAction` selbst — **Relay vor App deployen**.
