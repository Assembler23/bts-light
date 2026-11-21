# Das Zähltablett bedienen

Diese Seite ist für die Person **am Feld**: Zähltafelbediener, Schiedsrichter
oder wer sonst gerade das Tablet in der Hand hat. Sie erklärt, was das Gerät
kann und was in welcher Reihenfolge zu tun ist.

Wie der Tablet-Betrieb technisch aufgebaut ist, steht im
[Digitalen Tablet-Spielzettel](tablet.md). Hier geht es ums Bedienen.

> **Das Tablet gibt keinen Ton.** Weder Gong noch Sprachansage kommen aus dem
> Tablet — es ist ein Spielzettel, kein Lautsprecher. Aufrufe laufen über den
> Rechner der Turnierleitung.

## Mit einem Feld verbinden

1. Am Tablet den Browser öffnen und den **QR-Code des Feldes** scannen. Die
   Codes hängen an den Feldern oder werden von der Turnierleitung ausgedruckt.
2. Alternativ die Feld-Lobby öffnen und **das Feld aus der Liste wählen**.

Danach zeigt das Tablet entweder das laufende Spiel oder eine leere Ansicht,
bis die Turnierleitung ein Spiel auf das Feld legt.

**„Dieses Feld wird bereits geschiedst"** heißt: Ein anderes Gerät zählt hier
schon. Das ist eine Schutzmaßnahme — pro Feld zählt genau ein Tablet. Zum
Gerätetausch gibt es **Court übernehmen**; wer nur zuschauen will, wählt
**Nur Spielstand anzeigen**.
<!-- pruef: "Court übernehmen" in src-tauri/assets/tablet.html -->
<!-- pruef: "Nur Spielstand anzeigen" in src-tauri/assets/tablet.html -->
<!-- pruef: "Dieses Feld wird bereits geschiedst" in src-tauri/assets/tablet.html -->

**Die Kachel „Meetingpoint"** unter den Feldern ist nicht zum Zählen da: Sie
macht das Tablet (nach der PIN) zur Meetingpoint-Anzeige für die Spieler
und belegt kein Feld. Sie erscheint nur, wenn die Turnierleitung den
Meetingpoint eingeschaltet hat. Siehe [Meetingpoint](meetingpoint.md).
<!-- pruef: "PIN — antippen" in src-tauri/assets/lobby.html -->

## Üben am Demofeld

Wer das Tablet erst einmal ausprobieren möchte — etwa ein Schiedsrichter
vor seinem ersten Einsatz —, nimmt das **Demofeld** (seit v0.9.416). Es
geht **mitten im laufenden Turnier**, an jedem Turnier-Tablet:

- in der Feldwahl die Kachel **Demofeld** ganz unten (ohne PIN), oder
- über **⋯ Mehr** → **Feld wechseln · Anzeige …** in der Feldliste den
  Eintrag **Demofeld · Übungsspiel**.
<!-- pruef: "Demofeld · Übungsspiel" in src-tauri/assets/tablet.html -->
<!-- pruef: "Ausprobieren — antippen" in src-tauri/assets/lobby.html -->

Zuerst die **Zählweise**: **Sätze bis 21** oder **Sätze bis 15** (seit
v0.9.422). Dann wählst du ein **Übungsspiel**: Einzel, Doppel oder Mixed ab
0:0, oder einen Zwischenstand zum gezielten Üben — Entscheidungssatz
(Seitenwechsel bei 11 bzw. 8), Verlängerung ab 19:19 bzw. 13:13 oder ein
Spielpunkt bei 20:18 bzw. 14:12 bis zum Übermitteln des Ergebnisses. Alles
läuft wie im echten Spiel: Seitenwahl, Aufschlag, Pausen, Karten, Korrektur,
Schiri-Modus. Der **Ballzähler** ist auf dem Demofeld immer an.
<!-- pruef: "'Sätze bis {ziel}'" in src-tauri/assets/tablet.html -->
<!-- pruef: "ziel: 15, cap: 21, pause: 8" in src-tauri/assets/tablet.html -->
<!-- pruef: "Übungsspiel wählen" in src-tauri/assets/tablet.html -->
<!-- pruef: "Doppel · Verlängerung" in src-tauri/assets/tablet.html -->

**Am Turnier ändert das nichts.** Die Namen sind ausgedacht, kein echtes
Spiel wird genommen, kein Feld belegt. Nichts geht an die Turnierleitung,
an BTP, an die Monitore oder den Liveticker — auch „Ergebnis übermitteln"
und die Meldungen ✚ und 📣 bleiben auf dem Tablet. Ein großes **ÜBUNG** im
Hintergrund und „Übung · nichts wird übertragen" oben rechts erinnern daran.
<!-- pruef: "Übung · nichts wird übertragen" in src-tauri/assets/tablet.html -->

Auf dem Demofeld verlangt auch **Feld wechseln · Anzeige …** im Mehr-Menü
**keine PIN**: So lassen sich Schiri-Modus und Große Schrift frei
ausprobieren. **Neues Übungsspiel wählen** (ebenfalls unter **⋯ Mehr**)
führt zurück zur Auswahl. Zurück an ein echtes Feld geht es über die
Feldliste unter **Feld wechseln · Anzeige …**.
<!-- pruef: "Neues Übungsspiel wählen" in src-tauri/assets/tablet.html -->

## Warten auf das Spiel

Liegt noch kein Spiel auf dem Feld, steht groß der Feldname da und darunter
**„Warte auf das nächste Spiel"**. Zu tun ist nichts: Die Zähltafel startet
von selbst, sobald die Turnierleitung ein Spiel zuweist.
<!-- pruef: "Warte auf das nächste Spiel" in src-tauri/assets/tablet.html -->

Erreicht das Tablet den Turnier-PC länger als etwa zehn Sekunden nicht,
steht dort stattdessen **„Keine Verbindung zum Turnier-PC"**. Das Tablet
versucht es von selbst weiter. Dauert es länger: WLAN prüfen oder der
Turnierleitung Bescheid geben.
<!-- pruef: "Keine Verbindung zum Turnier-PC" in src-tauri/assets/tablet.html -->
<!-- pruef: /OFFLINE_HINWEIS_MS = 10_000;/ in src-tauri/assets/tablet.html -->

## Ein Spiel aufsetzen

Liegt ein Spiel auf dem Feld, zeigt die Kopfzeile eine **Aufruf-Uhr**:
„1. Aufruf", „2. Aufruf", „Letzter Aufruf" — je länger gewartet wird, desto
auffälliger. Sie läuft, bis die Aufstellung bestätigt ist, und sagt dir auf
einen Blick, ob hier gerade jemand vermisst wird.

Ein kurzer Assistent führt dann durch drei Fragen. Über jeder Frage steht, wo
du bist – zum Beispiel **„Schritt 2 von 3"**:
<!-- pruef: "t('Schritt {n} von {m}', schritt)" in src-tauri/assets/tablet.html -->

1. **Welches Team steht links?**
2. **Wer schlägt zuerst auf?**
3. **Wer nimmt an?** — nur im Doppel.

Aufschläger und Annehmer wählst du auf einem kleinen **Feldbild**: Jede
Mannschaft steht auf ihrer Hälfte, so wie du sie vor dir siehst, dazwischen
das Netz. Du tippst den Namen an. Bei der dritten Frage steht der gewählte
Aufschläger gelb markiert mit **„schlägt auf"** auf seiner Seite, den Annehmer
tippst du gegenüber an.
<!-- pruef: "schlägt auf" in src-tauri/assets/tablet.html -->
<!-- pruef: "setup-feld" in src-tauri/assets/tablet.html -->

Bei Doppel und Mixed wird **nach jedem Satz neu gefragt**, wer aufschlägt und
annimmt. Das ist gewollt: Die Aufstellung darf sich zwischen den Sätzen ändern.

### Anwesenheit bestätigen (Meetingpoint)

Hat die Turnierleitung es eingeschaltet, stehen bei der Seitenwahl die Namen
der Spieler (und des Schiedsrichters) unter **Anwesenheit bestätigen**. Wer
da ist, tippt seinen Namen an — der Haken geht an die Turnierleitung. Das ist
nur ein Hinweis für sie: Gezählt werden kann jederzeit, auch ohne
Bestätigung. Siehe [meetingpoint.md](meetingpoint.md).
<!-- pruef: "Anwesenheit bestätigen (antippen)" in src-tauri/assets/tablet.html -->

## Zählen

Getippt wird auf die Seite, die den Punkt gemacht hat. Das Tablet führt den
Aufschlagwechsel selbst nach — du musst nicht mitdenken, wer dran ist.

Auf dem Schirm steht dabei:

- die **Court-Grafik** mit den Namen auf ihren Hälften. Der **Federball**
  markiert den Aufschläger. Er liegt in dessen Aufschlagfeld, direkt hinter
  der vorderen Aufschlaglinie; sein Korken zeigt in Flugrichtung auf das
  diagonal gegenüberliegende Aufschlagfeld. Wechselt der Aufschlag, gleitet
  der Ball sichtbar ins neue Feld.
- die **Spieldauer** als Uhr in der Kopfzeile. Sie läuft ab dem Moment, in dem
  die Aufstellung bestätigt ist — nicht schon ab der Feldzuweisung.
- auf Wunsch **Vereinsname und Wappen** unter den Spielernamen. Das schaltet
  die Turnierleitung turnierweit; das Tablet übernimmt es mit der nächsten
  Zuweisung, ohne Neuladen.
- der Knopf **📈 Verlauf** unten rechts, mit dem Punktverlauf je Satz. Der
  wird lokal mitgeschrieben und **funktioniert auch ohne Netz**. Der Knopf
  erscheint mit dem ersten Punkt.
- oben rechts ein **Punkt** für die Verbindung: grün = verbunden. Auf
  kleineren Tablets steht bei bestehender Verbindung nur der Punkt; „verbinde…"
  und „getrennt" stehen immer als Text daneben.

**Dein Tipp hat gezählt, wenn die Ziffer aufleuchtet:** Die Zahl der Seite,
die den Punkt bekommen hat, springt kurz auf und leuchtet orange nach.

Steht eine Seite einen Punkt vor dem Satzgewinn, erscheint über dem Stand –
an der Stelle der Zeile „Satz 2 · best of 3 · bis 21" – eine Marke in ihrer
Farbe: **„Satzpunkt links"** oder „… rechts" (hinter dem Feld
„vorne"/„hinten"), bei 29:29 „Satzpunkt beide". Entschiede der Satz zugleich
das Spiel, heißt sie **„Spielpunkt …"**. Sie bleibt stehen, solange der
Satzpunkt besteht — du musst dafür nichts einschalten.
<!-- pruef: "'Spielpunkt' : 'Satzpunkt'" in src-tauri/assets/tablet.html -->
<!-- pruef: "seiteWort(lage.seite) : t('beide')" in src-tauri/assets/tablet.html -->

Ein Fehltipp lässt sich zurücknehmen — mit dem orangen Knopf **unten
links**. Es gibt nur diesen einen (seit v0.9.425; vorher waren es bis zu
drei), und er sagt, was er zurücknimmt: **„↩ Punkt für Links zurück"** oder
„… Rechts …" — zählst du hinter dem Feld, „Vorne" und „Hinten". Der Verlauf
wird dabei mit korrigiert.
<!-- pruef: "↩ Punkt für " in src-tauri/assets/tablet.html -->

Direkt nach der Aufstellung, vor dem ersten Punkt, heißt derselbe Knopf
**„↩ Aufstellung ändern"** und führt zurück in die Seiten- und
Aufschlagwahl.
<!-- pruef: "↩ Aufstellung ändern" in src-tauri/assets/tablet.html -->

**Im Entscheidungssatz** weist das Tablet beim Erreichen der Pausen-Schwelle
darauf hin, dass die **Seiten gewechselt** werden: In der Pause steht unter der
Überschrift ein oranges Band **„↔ Seitenwechsel"** — ebenso in jeder Satzpause.
<!-- pruef: "↔ Seitenwechsel — Spieler tauschen die Hälften" in src-tauri/assets/tablet.html -->

## Seiten oder Mannschaften korrigieren

Stimmt das Tablet nicht mehr mit dem Feld überein, tippst du oben rechts
auf **⋯ Mehr** und dort auf **⇄ Seiten / Mannschaften korrigieren**. Eine PIN
brauchst du dafür nicht. Der Dialog bietet zwei Wege:
<!-- pruef: "Seiten oder Mannschaften korrigieren" in src-tauri/assets/tablet.html -->

| Was ist passiert? | Knopf | Was danach gilt |
|---|---|---|
| Die Spieler stehen auf der anderen Hälfte als am Tablet — sie haben nicht gewechselt, obwohl das Tablet getauscht hat, oder umgekehrt | **Seiten tauschen** | Namen **und** Punkte wandern gemeinsam auf die andere Seite. Der Stand bleibt. |
| Bei der Aufstellung wurde die falsche Mannschaft als „links" gewählt; du hast aber richtig auf die Seite getippt | **Mannschaften tauschen** | Nur die Namen wechseln, die Punkte bleiben auf ihrer Seite. Der gemeldete Stand dreht sich — auch in den fertigen Sätzen. |
<!-- pruef: "Seiten tauschen" in src-tauri/assets/tablet.html -->
<!-- pruef: "Mannschaften tauschen" in src-tauri/assets/tablet.html -->

Vor dem Tausch zeigt eine **Rückfrage**, wer danach wo steht, welcher Stand an
die Turnierleitung geht und — bei „Mannschaften tauschen" am Spielende — wer
dann gewonnen hat. Erst **„Ja, tauschen"** führt es aus.
<!-- pruef: "Ja, tauschen" in src-tauri/assets/tablet.html -->

Gut zu wissen:

- **Im Doppel** fragt das Tablet nach „Mannschaften tauschen" sofort, **wer
  gerade aufschlägt und wer annimmt** — welcher Spieler wo steht, weiß es nach
  dem Tausch nicht. Der Stand bleibt dabei.
- **„↩ Korrektur zurücknehmen"** steht danach auf dem Zurück-Knopf unten
  links und macht den Tausch rückgängig. Erst danach nimmt der Knopf wieder
  Punkte zurück.
<!-- pruef: "Korrektur zurücknehmen" in src-tauri/assets/tablet.html -->
- **Im Entscheidungssatz vor 11** wechseln die Seiten bei 11 trotzdem noch
  einmal — die Rückfrage weist darauf hin. Korrigiert wird nur, was am Tablet
  nicht zum Feld passt.
- **In der Pause und am Spielende** verdeckt die Anzeige die Kopfzeile samt
  „⋯ Mehr". Dort steht dafür ein eigener Knopf
  **„⇄ Seiten / Mannschaften korrigieren"**.
- **Nach dem Übermitteln** geht es am Tablet nicht mehr — dann ist die
  Turnierleitung dran.
- „Mannschaften tauschen" fehlt, solange der Turnier-PC noch eine ältere
  Version hat.

## Bälle zählen

Hat die Turnierleitung den Ballzähler eingeschaltet, steht in der Kopfzeile
eine kleine Gruppe: **−**, ein Federball mit der Zahl (anfangs 0) und **+**. Tippe bei jedem neuen Ball auf **+**, auch
beim ersten. Hast du dich vertippt, nimmt **−** einen Ball zurück.
<!-- pruef: "Ein Ball mehr" in src-tauri/assets/tablet.html -->
<!-- pruef: "Ein Ball weniger" in src-tauri/assets/tablet.html -->

- Eine PIN brauchst du dafür nicht.
- „Punkt zurück" (↩) ändert die Ballzahl nicht.
- Die Zahl bleibt auch nach einer Pause, nach dem Neuladen und nach einem
  Tablet-Wechsel stehen. Höchstens 99.
  <!-- pruef: "const BALL_MAX = 99;" in src-tauri/assets/tablet.html -->
- Sobald du das Ergebnis übermittelst, geht die Zahl mit an die
  Turnierleitung. Danach lässt sie sich am Tablet nicht mehr ändern, außer du
  öffnest das Ergebnis wieder.
- Siehst du die Gruppe nicht, zählt dieses Turnier die Bälle nicht am Tablet.

## Einspielzeit

Ist die Seiten- und Aufschlagwahl abgeschlossen, zeigt das Tablet bis zum
ersten Ballwechsel groß die **Einspielzeit** — die Zeit seit der Wahl. Nach
**2 Minuten** wird sie rot.
<!-- pruef: "Einspielzeit" in src-tauri/assets/tablet.html -->
<!-- pruef: /EINSPIEL_SEKUNDEN = 120;/ in src-tauri/assets/tablet.html -->

Darunter steht der Knopf **Spiel beginnt**. Tippe ihn, sobald der erste
Aufschlag gespielt wird: Die Einspielzeit verschwindet, und die
Turnierleitung misst die reine Spielzeit ab diesem Moment. Vergisst du den
Knopf, ist das nicht schlimm — dann verschwindet die Einspielzeit mit dem
ersten Punkt, und die Spielzeit zählt ab dem Ende des ersten Ballwechsels.
<!-- pruef: "Spiel beginnt" in src-tauri/assets/tablet.html -->

## Pausen

Das Tablet blendet die offiziellen Pausen selbst ein:

| Wann | Dauer |
|---|---|
| Beim Erreichen der **Punktepausen-Schwelle** im Satz — bei 21er-Sätzen sind das 11 Punkte | 60 Sekunden |
| **Zwischen zwei Sätzen** | 2 Minuten |

Die Schwelle kommt **aus dem Spielformat**, nicht aus einer festen Regel im
Tablet: Bei kürzeren Sätzen liegt sie entsprechend tiefer, und bei Formaten
ohne vorgesehene Punktepause gibt es sie schlicht nicht. Die Überschrift der
Pause nennt die jeweils geltende Zahl.

Während der Pause ist die Zähltafel **gesperrt** — es lässt sich nicht
versehentlich weiterzählen.

**Aus Versehen ausgelöst?** In der Pause steht derselbe Rückgängig-Knopf wie
unter dem Feld – **„↩ Punkt für Links zurück"**, nach einem Satz **„↩ Letzten
Punkt zurück"**. Das bricht die Pause ab und nimmt den Punkt zurück, der sie
ausgelöst hat — der Fall „Ball wird wiederholt".

**Aufschlag schon in der Satzpause festlegen (Doppel).** Zwischen zwei Sätzen
steht im Pausenbild **„Aufschlag & Annahme jetzt festlegen"**. Damit lassen sich
Aufschläger und Annehmer des nächsten Satzes eintragen, während die Pause
weiterläuft; die Restzeit steht dabei oben im Auswahlfenster. **„← Zurück zur
Pause"** schließt die Auswahl ohne Wahl — dann fragt das Tablet wie gewohnt nach
**Weiterspielen**.
<!-- pruef: "Aufschlag &amp; Annahme jetzt festlegen" in src-tauri/assets/tablet.html -->
<!-- pruef: "Zurück zur Pause" in src-tauri/assets/tablet.html -->

**Die Pause endet nicht von allein.** Läuft der Countdown ab, zählt die
Anzeige rot weiter („überzogen +0:37"), bis jemand **Weiterspielen** tippt.
Das ist Absicht: Die Turnierleitung sieht dadurch feldgenau, wo es hakt.
Ein Neuladen oder ein Gerätetausch behält die laufende Pause bei.

## Der Turnierleitung etwas melden

Zwei Meldungen gehen vom Tablet an die Turnierleitung, beide ohne PIN:

- **✚ Verletzung · Behandlung** — steht unter **⋯ Mehr** (seit v0.9.425;
  vorher ein eigener Knopf in der Kopfzeile). Unterbricht das Spiel. Eine
  große Uhr zählt
  die **Behandlungspause** hoch, darunter steht die Richtzeit des Turniers
  (Standard 5 Minuten). Ist sie erreicht, wird die Uhr rot und der Titel lautet
  **„Behandlungspause — Richtzeit überschritten"** — die Uhr zählt weiter, die
  Pause endet erst mit **Weiterspielen**. Das Feld wird bei der Turnierleitung
  rot hervorgehoben. Beginn, Ende und Dauer der Behandlung stehen automatisch
  auf dem Schiedsrichterzettel.
  <!-- pruef: "Richtzeit überschritten" in src-tauri/assets/tablet.html -->
- **✚ auch in der Pause:** Passiert die Verletzung in der Pause bei 11 oder in
  der Satzpause, verdeckt die Pausenanzeige die Kopfzeile. Dafür steht dort der
  Knopf **„✚ Verletzung · Behandlung"**; er macht aus der Pause eine
  Behandlungspause. Nach Spielende gibt es den Eintrag nicht mehr.
  <!-- pruef: "✚ Verletzung · Behandlung" in src-tauri/assets/tablet.html -->
- **📣 Turnierleitung** — der Knopf oben rechts (auf schmalen Geräten nur
  das Zeichen 📣). Er meldet, dass jemand ans Feld kommen soll. Es erscheint
  eine Rückfrage, damit nichts aus Versehen ausgelöst wird. Der Knopf
  erscheint, sobald die Aufstellung bestätigt ist — ohne Spiel und während
  der Seitenwahl fehlt er. Die Turnierleitung sieht den Ruf am Turnier-PC und
  auf ihren eigenen Geräten oben als Meldung mit der Feldnummer. Hat sich die
  Sache erledigt, den Knopf noch einmal antippen und **Meldung zurücknehmen**
  wählen — sie verschwindet dann überall.
  <!-- pruef: "Meldung zurücknehmen" in src-tauri/assets/tablet.html -->

  Die Turnierleitung kann den Ruf auch von sich aus zurücknehmen — etwa wenn
  schon jemand unterwegs ist. Dann geht das Megafon von allein aus, und unten
  steht kurz **„Die Turnierleitung hat den Ruf zurückgenommen."** Braucht ihr
  doch noch jemanden, einfach erneut rufen.
  <!-- pruef: "Die Turnierleitung hat den Ruf zurückgenommen." in src-tauri/assets/tablet.html -->
  <!-- pruef: /<span class="txt" data-t>Turnierleitung<\/span>/ in src-tauri/assets/tablet.html -->

Beide Meldungen erscheinen bei der Turnierleitung in einer Leiste, die auf
jeder Seite sichtbar ist — mit Feldnummer. **Aufgelöst werden sie am Tablet**,
nicht am PC: durch „Weiterspielen" beziehungsweise durch Zurücknehmen der
Meldung.

## Das Mehr-Menü

Oben rechts sitzt **⋯ Mehr** (auf schmalen Geräten nur „⋯"). Dahinter steckt
alles, was man selten braucht (seit v0.9.425 — vorher lag ein Teil davon in
Kopf- und Fußzeile, der Rest hinter einem Zahnrad mit PIN). Das Menü öffnet
**ohne PIN** und zeigt nur, was gerade geht:

| Abschnitt | Einträge |
|---|---|
| **Spiel** | **Ergebnis eintragen …** · **Spiel beenden …** · **⇄ Seiten / Mannschaften korrigieren** · **✚ Verletzung · Behandlung** |
| **Schiedsrichter** | **Schiri-Modus: an/aus** — ohne PIN |
| **Sprache · Language** | **Deutsch** · **English** — ohne PIN |
| **Gerät** | **Feld wechseln · Anzeige …** — hinter der PIN |
<!-- pruef: "Ergebnis eintragen …" in src-tauri/assets/tablet.html -->
<!-- pruef: "Schiri-Modus: " in src-tauri/assets/tablet.html -->
<!-- pruef: "Feld wechseln · Anzeige …" in src-tauri/assets/tablet.html -->

Solange kein Spiel gezählt wird, fehlt der Abschnitt „Spiel". Auf breiten
Tablets stehen die Einträge zweispaltig; passt das Menü nicht auf den Schirm,
lässt es sich verschieben. **Schließen** steht als Rahmen-Knopf ganz unten.

### Sprache: Deutsch oder English

Das Tablet spricht Deutsch oder Englisch (seit v0.9.425). Umgestellt wird
unter **⋯ Mehr** im Abschnitt **Sprache · Language**: **Deutsch** oder
**English** antippen — ohne PIN, auch mitten im Spiel. Der Stand, die Uhr und
eine laufende Pause bleiben dabei, wie sie sind.
<!-- pruef: "Sprache · Language" in src-tauri/assets/tablet.html -->
<!-- pruef: /id="sprache-en">English</ in src-tauri/assets/tablet.html -->

- Die Wahl gilt **für dieses Tablet** und bleibt gespeichert — auch nach
  einem Feldwechsel und über Nacht. Andere Tablets und die Monitore ändern
  sich nicht.
- **Deutsch bleiben** die Ansagetexte des Schiri-Modus (sie werden in der
  Halle vorgelesen), die Namen von Feld, Klasse und Spielern und ein
  Ablehnungsgrund, den die Turnierleitung zurückschickt.
- Bis v0.9.422 standen einige Knöpfe in beiden Sprachen zugleich
  („Zurück · Back"). Das gibt es nicht mehr.

### Gerät — hinter der PIN

Das Feld wechseln und die Anzeige umstellen soll niemand aus Versehen. Dieser
Teil ist deshalb durch eine **PIN** geschützt (voreingestellt `0000`, die
Turnierleitung kennt sie). Solange die PIN verlangt wird, trägt der Eintrag
ein Schloss 🔒.

**Feld wechseln** — ohne einen QR-Code zu scannen.

**Anzeige:**

- **Anzeige (nur Spielstand)** — macht das Gerät zur reinen Zähltafel,
- **Anordnung: …** — automatisch, nebeneinander (links–rechts) oder
  übereinander (vorn–hinten, für den Platz hinter dem Feld). In der
  Anzeige-Hülle heißt der entsprechende Eintrag **Ansicht: …** (seit
  v0.9.292) und schaltet Seiten und Anordnung in einem Zug; dort genügt auch
  ein **Tipp auf die Punktzahlen**, ohne PIN.
<!-- pruef: "Anordnung: " in src-tauri/assets/tablet.html -->
<!-- pruef: "Ansicht: " in src-tauri/assets/anzeige.html -->
- **Große Schrift: an/aus** (seit v0.9.414) — für alle, die schlecht sehen:
  größere Ansagetexte, Schwarz auf Weiß, kräftigere Spielernamen. Die
  Einstellung bleibt auf diesem Tablet gespeichert,
<!-- pruef: "Große Schrift: " in src-tauri/assets/tablet.html -->
- **Vollbild**.

> **Die PIN wird fünf Minuten gemerkt** (seit v0.9.284). Nach einer richtigen
> Eingabe öffnet „Feld wechseln · Anzeige …" fünf Minuten lang ohne erneute
> Eingabe — auch über einen Feldwechsel und „Neu laden" hinweg. Die Frist
> läuft ab der Eingabe und verlängert sich **nicht** von selbst. Praktisch,
> wenn du mehrere Felder hintereinander einrichtest.
<!-- Fuenf Minuten: FREIGABE_MS in der Regel. Aendert sich die Frist,
     steht hier eine falsche Zahl. -->
<!-- pruef: /FREIGABE_MS = 5 \* 60 \* 1000/ in src/io/pinFreigabe.mjs -->

## Karten und Verwarnungen

Wer als Schiedsrichter zählt, hat zusätzlich die Karten zur Hand — gelb, rot,
schwarz — samt der vorzulesenden Ansagen.

> **Der Schiri-Modus muss erst eingeschaltet werden** — unter **⋯ Mehr**,
> ohne PIN (seit v0.9.425). Ohne ihn erscheinen die Karten gar nicht.

Die Ansage zum Vorlesen steht so groß da, wie der Platz es zulässt — kurze
Ansagen größer als lange. Reicht das nicht, hilft **Große Schrift** unter
**⋯ Mehr** → **Feld wechseln · Anzeige …**.

Details: [Schiri-Modus am Zähltablett](umpire-mode.md).

## Ein Spiel beenden

### Regulär

Nach dem letzten Punkt zeigt das Tablet das Spielende: den Sieger und darunter
eine kleine **Tabelle** – eine Zeile je Mannschaft, die Sätze als Spalten, der
Sieger oben. Gewonnene Sätze sind hell, verlorene blasser (ein durch Aufgabe
abgebrochener Satz hat keinen Gewinner und bleibt blass). Darunter steht groß
**„Ergebnis übermitteln"**; **„Spiel wieder öffnen"** ist bewusst nur ein
schmal umrandeter Knopf, damit er nicht aus Versehen getroffen wird. Er nimmt
den letzten Schritt zurück – meist den letzten Punkt –, und das Spiel läuft
weiter.
<!-- pruef: "Spiel wieder öffnen" in src-tauri/assets/tablet.html -->

> **Das Ergebnis geht nicht von allein weg.** Du musst
> **„Ergebnis übermitteln"** tippen. Erst danach wandert es zur Turnierleitung
> und in den Tournament Planner.
<!-- pruef: "Ergebnis übermitteln" in src-tauri/assets/tablet.html -->

Danach siehst du eines von vier Dingen:

| Anzeige | Bedeutung |
|---|---|
| „Ergebnis wird übermittelt … wird automatisch wiederholt" | unterwegs; bei Netzproblemen versucht es das Tablet von allein weiter |
| **✓ Übermittelt** | angekommen — das Feld ist fertig |
| **✗ Nicht angenommen: …** | abgelehnt, mit Begründung. Der Knopf wird wieder frei: **Stand prüfen** und erneut übermitteln |

| **✓ In BTP steht für dieses Spiel schon ein Ergebnis** | die Turnierleitung hat bereits eines eingetragen — nichts mehr zu tun. Weicht deins ab, sag es ihr |
<!-- pruef: "In BTP steht für dieses Spiel schon ein Ergebnis" in src-tauri/assets/tablet.html -->

Bleib am Feld, bis eines dieser Endergebnisse dasteht. Ein „✗ Nicht
angenommen", das niemand liest, ist ein verlorenes Ergebnis.

### Das orange Band: ein Ergebnis ist nicht angekommen

Kommt das nächste Spiel aufs Feld (oder wird das Feld geleert), **bevor** das
Ergebnis des alten Spiels zugestellt ist, geht es nicht verloren und blockiert
auch nichts (seit v0.9.376). Das Tablet zeigt dann ganz unten ein oranges Band:

> Ergebnis von Spiel Nr. 12 (21:10, 21:15) nicht zugestellt — Turnierleitung informieren
<!-- pruef: "nicht zugestellt — Turnierleitung informieren" in src-tauri/assets/tablet.html -->

- **Sag der Turnierleitung Bescheid** und nenne Spielnummer und Sätze — sie
  stehen im Band. Das neue Spiel kannst du ganz normal zählen und übermitteln.
- Das Tablet versucht es im Hintergrund weiter. Kommt das Ergebnis doch noch
  an, oder hat die Turnierleitung es inzwischen von Hand eingetragen, wird das
  Band kurz grün („… ist zugestellt" bzw. „… nicht mehr nötig — in BTP steht
  schon eines") und verschwindet.
- Das Band übersteht ein Neuladen der Seite.
- **Verwerfen** entfernt das Band. Das Tablet fragt vorher noch einmal nach
  und zeigt das Ergebnis — verwirf nur, wenn die Turnierleitung es hat.
<!-- pruef: "Ja, verwerfen" in src-tauri/assets/tablet.html -->

Kommt **dasselbe Spiel** später wieder auf dieses Feld, sendet das Tablet
nichts von allein. Es fragt: *„Für dieses Spiel liegt ein nicht zugestelltes
Ergebnis vor"* — **Jetzt senden** oder **Verwerfen** (auch hier mit
Nachfrage). Wird das Spiel neu gespielt, verwirf; soll das alte Ergebnis
gelten, sende. Nach „Jetzt senden" bleibt das Band stehen, bis der Turnier-PC
das Ergebnis angenommen hat.
<!-- pruef: "Jetzt senden" in src-tauri/assets/tablet.html -->

> **Steht das Ergebnis in BTP fest, ist am Tablet Schluss** (seit v0.9.291).
> Die Beendet-Ansicht sperrt dann „Spiel wieder öffnen" **und** den
> Sende-Knopf und sagt: *„Ergebnis steht in BTP fest — Änderung nur über die
> Turnierleitung."* Vorher ließ sich das Spiel wieder öffnen, und das erneute
> Übermitteln wurde danach stillschweigend verworfen — am 12.09.2026 auf Feld 11
> 23-mal, ohne ein Wort auf dem Schirm.
<!-- pruef: "Ergebnis steht in BTP fest" in src-tauri/assets/tablet.html -->

### Vorzeitig — „Spiel beenden …"

Unter **⋯ Mehr** steht **„Spiel beenden …"** (bis v0.9.422 „Match beenden …"
in der Fußzeile). Der Eintrag öffnet einen Dialog mit **drei** Wegen — die
Wahl hat unterschiedliche Folgen:
<!-- pruef: "Spiel beenden …" in src-tauri/assets/tablet.html -->
<!-- pruef: "Kampflos" in src-tauri/assets/tablet.html -->

| Wahl | Wirkung |
|---|---|
| **Aufgabe — nur dieses Spiel** | Dieses Spiel wird als Aufgabe gewertet. Alles andere läuft weiter. |
| **Verletzung — auch Folgespiele der Disziplin** | Zusätzlich werden die **restlichen Spiele dieser Disziplin** kampflos gewertet. |
| **Kampflos** | Eine Partei ist nicht angetreten. Geht ab 0:0. |

> **Die mittlere Wahl ist die folgenreichste am ganzen Tablet.** Sie wirkt über
> das aktuelle Spiel hinaus auf die gesamte Disziplin. Im Zweifel die erste
> Variante wählen und die Turnierleitung fragen.

Bei Aufgabe und kampfloser Wertung musst du zuerst den **Sieger** wählen — bis
dahin bleibt „Ergebnis übermitteln" gesperrt und weist darauf hin.

Kampflos lässt sich also **am Tablet** werten. Wie die Turnierleitung dieselbe
Sache von ihrer Seite aus behandelt — samt der Frage, was mit Folgespielen in
Gruppen passiert — steht in
[Kampflose Wertung nach Aufgabe](walkover.md).

### Aufgabe aus der Behandlungspause

Läuft gerade eine Behandlungspause, gibt es dort zusätzlich **„Spiel
abbrechen"**. Der laufende Satz wird als Teilstand übernommen (etwa 21:10,
dann 5:5), danach wählst du den Sieger.

### Wenn niemand gezählt hat

Wurde auf Papier gezählt oder das Tablet gar nicht benutzt, lässt sich der
**Endstand direkt eintragen**, ohne den Spielverlauf nachzuklopfen. Läuft das
Spiel noch, bietet derselbe Dialog außerdem den **Einstieg in ein laufendes
Spiel** an — du übernimmst dann mit dem aktuellen Stand und zählst weiter.

**Du tippst nur die Punkte des Verlierers.** Das Tablet ergänzt den Sieger
selbst: aus 11 wird 21:11, aus 20 wird 22:20. Der ergänzte Wert ist gelb
hinterlegt – stimmt er nicht, tippst du ihn einfach über; was du selbst
eingetragen hast, ändert das Tablet nie (eine 0 zählt als leer). Tippst du die Punkte des Siegers (21
oder mehr), ergänzt es nichts, denn dann ist der Gegner offen.
<!-- pruef: "es-vorschlag" in src-tauri/assets/tablet.html -->
<!-- pruef: "if (punkte < target - 1) return target;" in src-tauri/assets/tablet.html -->

## Das Tablet wechseln

Fällt ein Gerät aus oder ist der Akku leer, öffnet das Ersatzgerät dasselbe
Feld und tippt **Court übernehmen**. Es setzt **mit dem aktuellen Spielstand** fort;
das alte Gerät wird gesperrt. Es geht nichts verloren, weil das zählende Tablet
seinen Stand laufend an den Turnier-PC spiegelt.

**Das alte Gerät ist einfach aus?** Dann ist das Feld frei, und das
Ersatzgerät öffnet es ohne Dialog. Es übernimmt den gesicherten Spielstand
trotzdem — samt den letzten „zurück"-Schritten (seit v0.9.377; vorher begann
es bei 0:0).

**Nach einem Neustart des Turnier-PCs** kennt er nur noch die Sätze, nicht
mehr die Aufstellung. Das Ersatzgerät steigt dann mit diesem Satzstand ein
und fragt wie beim Spielbeginn: welches Team links steht und wer gerade
aufschlägt. Der Zurück-Knopf reicht in diesem Fall nicht hinter den
Einstieg.
<!-- pruef: "Wer schlägt gerade auf?" in src-tauri/assets/tablet.html -->

**Kurze Netzaussetzer sind keine Übernahme.** Verliert dasselbe Tablet
kurzzeitig das WLAN und meldet sich zurück, macht es nahtlos weiter — ohne
Dialog und ohne Tippen. Den Übernahme-Dialog sieht nur ein **fremdes** Gerät.

## Akkustand

**Der Akkustand steht auf dem Tablet selbst** (seit v0.9.293) — klein in der
Kopfzeile beim Zählen, in der Feldwahl und in der Anzeige-Hülle neben dem
Zahnrad. Im Kiosk-Betrieb fehlt die Android-Statusleiste; vorher sah man den
Stand nur am Turnier-PC.

| Anzeige | Bedeutung |
|---|---|
| `🔋 73 %` | Ladestand |
| `⚡` | hängt am Netz |
| **orange** | unter 20 % |
| **rot** | unter 10 % |
<!-- pruef: "🔋" in src-tauri/assets/tablet.html -->

Die Turnierleitung sieht denselben Wert in ihrer Feldübersicht. Abgefragt wird
er einmal je Minute — die Anzeige kostet also selbst keinen Akku.

Dass sie **fehlt**, ist kein Fehler: Es braucht eine Quelle für den Wert.

- **iPads geben den Akkustand grundsätzlich nicht heraus** — dort bleibt die
  Anzeige immer weg.
- **Android-Tablets brauchen eine Quelle:** am zuverlässigsten die Kiosk-App
  beziehungsweise der Kiosk-Browser, die den Wert selbst bereitstellen. Ohne sie
  greift nur die Browser-Schnittstelle, und die liefert über eine gewöhnliche
  unverschlüsselte Verbindung im Hallen-WLAN nichts. Siehe
  [Zähl-Tablet als Kiosk-App (Android)](tablet-android-app.md) und
  [Einstellungs-PIN & Kiosk-Sperre](tablet-kiosk.md).

## Was das Tablet nicht kann

- **Kein Ton** — siehe oben.
- **Kein Zugriff auf Auslosung oder Zeitplan** — das Tablet zeigt genau ein
  Spiel auf genau einem Feld.
- **Das Spielformat wird nicht am Tablet eingestellt.** Satzzahl und
  Zielpunktzahl kommen je Spiel aus dem Tournament Planner; das Tablet zeigt
  sie in der Satzzeile an („Satz 1 · best of 3 · bis 21"). Neben 21er-Sätzen
  sind auch kürzere Formate und Zeitspiele möglich.

## Wenn etwas nicht geht

**Das Tablet findet das Feld nicht / die Seite lädt nicht.**
Prüfe zuerst, ob das Tablet im richtigen WLAN ist. Erreicht es den Turnier-PC
nicht, taucht es auch in dessen Feldübersicht nicht als verbunden auf — das ist
für die Turnierleitung die schnellste Kontrolle. Häufigste Ursachen: falsches
WLAN, oder die Windows-Firewall blockiert den Zugang.

**Auf einem Android-Handy lädt die Seite in Chrome nicht
(„ERR_TOO_MANY_RETRIES"), in Firefox geht es.**
Neuere Android-Versionen verlangen für Chrome eine eigene Erlaubnis, Geräte
im lokalen Netz zu erreichen — ohne sie kommt Chrome nicht zum Turnier-PC
und meldet nur diesen Fehler (gesehen auf Android 17 mit Chrome 156). Abhilfe:
Android-**Einstellungen → Apps → Chrome → Berechtigungen**, dort **Geräte in
der Nähe** bzw. **Lokales Netzwerk** zulassen und Chrome neu starten. Chrome
fragt beim ersten Aufruf auch selbst — wurde das abgelehnt, geht es nur noch
über die Einstellungen. Zähl-Tablets mit der bts-light-Kiosk-App betrifft das
nach bisherigem Stand nicht.

**„Dieses Feld wird bereits geschiedst", obwohl niemand zählt.**
Meist hängt noch eine alte Sitzung eines anderen Geräts. **Court übernehmen**
löst das auf.

**„Feld wechseln · Anzeige …" verlangt eine PIN.**
Dieser Teil des Mehr-Menüs ist geschützt, damit niemand aus Versehen das Feld
wechselt. Alles andere im Menü geht ohne PIN. Die PIN kennt die
Turnierleitung; voreingestellt ist `0000`. Siehe
[Einstellungs-PIN & Kiosk-Sperre](tablet-kiosk.md).

**Die Pause läuft rot weiter.**
Das ist kein Fehler, sondern gewollt — sie wartet auf **Weiterspielen**.

**Das Tablet reagiert nach einem Punkt kurz nicht.**
Nach jedem gezählten Punkt nimmt die Zähltafel für einen Sekundenbruchteil
keinen weiteren Tipp an. Das verhindert, dass ein hektischer Doppeltipp zwei
Punkte vergibt. Man sieht die Sperre nicht (bis v0.9.422 wurden die Flächen
dabei blass) — ob dein Tipp gezählt hat, zeigt die **aufleuchtende Ziffer**.
Kurz warten und normal weitertippen — es ist nichts verloren.
<!-- pruef: "var SCORE_COOLDOWN_MS = 700;" in src-tauri/assets/tablet.html -->

**„Ergebnis übermitteln" bleibt gesperrt.**
Bei Aufgabe und kampfloser Wertung muss zuerst der **Sieger** gewählt werden.
Der Hinweis dazu steht direkt am Knopf.

**„Ergebnis übermitteln" tat am neuen Spiel nichts.**
Das lag bis v0.9.375 an einem alten, nie zugestellten Ergebnis. Seit v0.9.376
wird es geparkt (oranges Band unten) und sperrt den Knopf nicht mehr.

**Ein Hinweis auf eine neue Fassung erscheint mitten im Spiel.**
Das Tablet lädt sich **nicht** von allein neu, solange gezählt wird. Den
Hinweis kannst du bis zum Spielende stehen lassen.

## Weiterlesen

- [Digitaler Tablet-Spielzettel](tablet.md) — wie der Betrieb aufgebaut ist
- [Schiri-Modus am Zähltablett](umpire-mode.md) — Karten und Ansagen
- [Einstellungs-PIN & Kiosk-Sperre](tablet-kiosk.md) — Gerät gegen Verstellen sichern
- [Punktverlauf-Graph](punktverlauf.md) — was der Verlauf zeigt
- [Zähltafelbediener einteilen](zaehltafelbediener.md) — wer wann ans Feld kommt
