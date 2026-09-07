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

## Ein Spiel aufsetzen

Liegt ein Spiel auf dem Feld, zeigt die Kopfzeile eine **Aufruf-Uhr**:
„1. Aufruf", „2. Aufruf", „Letzter Aufruf" — je länger gewartet wird, desto
auffälliger. Sie läuft, bis die Aufstellung bestätigt ist, und sagt dir auf
einen Blick, ob hier gerade jemand vermisst wird.

Ein kurzer Assistent führt dann durch drei Fragen:

1. **Welches Team steht links?**
2. **Wer schlägt zuerst auf?**
3. **Wer nimmt an?** — nur im Doppel.

Bei Doppel und Mixed wird **nach jedem Satz neu gefragt**, wer aufschlägt und
annimmt. Das ist gewollt: Die Aufstellung darf sich zwischen den Sätzen ändern.

## Zählen

Getippt wird auf die Seite, die den Punkt gemacht hat. Das Tablet führt den
Aufschlagwechsel selbst nach — du musst nicht mitdenken, wer dran ist.

Auf dem Schirm steht dabei:

- die **Court-Grafik** mit den Namen auf ihren Hälften. Der **Federball**
  markiert den Aufschläger; sein Korken zeigt in Flugrichtung auf das
  diagonal gegenüberliegende Aufschlagfeld.
- die **Spieldauer** als Uhr in der Kopfzeile. Sie läuft ab dem Moment, in dem
  die Aufstellung bestätigt ist — nicht schon ab der Feldzuweisung.
- auf Wunsch **Vereinsname und Wappen** unter den Spielernamen. Das schaltet
  die Turnierleitung turnierweit; das Tablet übernimmt es mit der nächsten
  Zuweisung, ohne Neuladen.
- der Knopf **📈 Verlauf** mit dem Punktverlauf je Satz. Der wird lokal
  mitgeschrieben und **funktioniert auch ohne Netz**.

Ein Fehltipp lässt sich zurücknehmen. Der Verlauf wird dabei mit korrigiert.

**Im Entscheidungssatz** weist das Tablet beim Erreichen der Pausen-Schwelle
darauf hin, dass die **Seiten gewechselt** werden.

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

**Aus Versehen ausgelöst?** In der Pause gibt es **„↩ Korrektur — letzter Punkt
zurück"**. Das bricht die Pause ab und nimmt den Punkt zurück, der sie
ausgelöst hat — der Fall „Ball wird wiederholt".

**Die Pause endet nicht von allein.** Läuft der Countdown ab, zählt die
Anzeige rot weiter („überzogen +0:37"), bis jemand **Weiterspielen** tippt.
Das ist Absicht: Die Turnierleitung sieht dadurch feldgenau, wo es hakt.
Ein Neuladen oder ein Gerätetausch behält die laufende Pause bei.

## Der Turnierleitung etwas melden

Oben rechts sitzen zwei Knöpfe. Sie tragen **nur die Zeichen** ✚ und 📣 — die
Beschriftung erscheint erst, wenn man darauf zeigt:

- **✚ Verletzung/Behandlung** — unterbricht das Spiel. Es läuft eine
  Behandlungspause **ohne Countdown**, bis **Weiterspielen** getippt wird. Das
  Feld wird bei der Turnierleitung rot hervorgehoben.
- **📣 Turnierleitung rufen** — meldet, dass jemand ans Feld kommen soll. Es
  erscheint eine Rückfrage, damit nichts aus Versehen ausgelöst wird.

Beide Meldungen erscheinen bei der Turnierleitung in einer Leiste, die auf
jeder Seite sichtbar ist — mit Feldnummer. **Aufgelöst werden sie am Tablet**,
nicht am PC: durch „Weiterspielen" beziehungsweise durch Zurücknehmen der
Meldung.

## Das Zahnrad-Menü

Hinter dem Zahnrad stecken die Dinge, die man selten braucht und deshalb nicht
aus Versehen antippen soll. Es ist durch eine **PIN** geschützt (voreingestellt
`0000`, die Turnierleitung kennt sie):

- **Feld wechseln**, ohne einen QR-Code zu scannen,
- **Anzeige (nur Spielstand)** — macht das Gerät zur reinen Zähltafel,
- **Schiri-Modus ein- und ausschalten**,
- **Vollbild**.

## Karten und Verwarnungen

Wer als Schiedsrichter zählt, hat zusätzlich die Karten zur Hand — gelb, rot,
schwarz — samt der vorzulesenden Ansagen.

> **Der Schiri-Modus muss erst eingeschaltet werden**, im Zahnrad-Menü. Ohne
> ihn erscheinen die Karten gar nicht.

Details: [Schiri-Modus am Zähltablett](umpire-mode.md).

## Ein Spiel beenden

### Regulär

Nach dem letzten Punkt zeigt das Tablet das Spielende.

> **Das Ergebnis geht nicht von allein weg.** Du musst
> **„Ergebnis übermitteln"** tippen. Erst danach wandert es zur Turnierleitung
> und in den Tournament Planner.
<!-- pruef: "Ergebnis übermitteln" in src-tauri/assets/tablet.html -->

Danach siehst du eines von drei Dingen:

| Anzeige | Bedeutung |
|---|---|
| „Ergebnis wird übermittelt … wird automatisch wiederholt" | unterwegs; bei Netzproblemen versucht es das Tablet von allein weiter |
| **✓ Übermittelt** | angekommen — das Feld ist fertig |
| **✗ Nicht angenommen: …** | abgelehnt, mit Begründung. Der Knopf wird wieder frei: **Stand prüfen** und erneut übermitteln |

Bleib am Feld, bis eines der beiden Endergebnisse dasteht. Ein „✗ Nicht
angenommen", das niemand liest, ist ein verlorenes Ergebnis.

### Vorzeitig — der Knopf „Match beenden …"

In der Fußzeile sitzt **„Match beenden …"**. Er öffnet einen Dialog mit **drei**
Wegen — die Wahl hat unterschiedliche Folgen:
<!-- pruef: "Match beenden" in src-tauri/assets/tablet.html -->
<!-- pruef: "Kampflos" in src-tauri/assets/tablet.html -->

| Wahl | Wirkung |
|---|---|
| **Aufgabe — nur dieses Spiel** | Dieses Spiel wird als Aufgabe gewertet. Alles andere läuft weiter. |
| **Verletzung — auch Folgespiele der Disziplin** | Zusätzlich werden die **restlichen Spiele dieser Disziplin** kampflos gewertet. |
| **Kampflos · Walkover** | Eine Partei ist nicht angetreten. Geht ab 0:0. |

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

## Das Tablet wechseln

Fällt ein Gerät aus oder ist der Akku leer, öffnet das Ersatzgerät dasselbe
Feld und tippt **Court übernehmen**. Es setzt **mit dem aktuellen Spielstand** fort;
das alte Gerät wird gesperrt. Es geht nichts verloren, weil das zählende Tablet
seinen Stand laufend an den Turnier-PC spiegelt.

**Kurze Netzaussetzer sind keine Übernahme.** Verliert dasselbe Tablet
kurzzeitig das WLAN und meldet sich zurück, macht es nahtlos weiter — ohne
Dialog und ohne Tippen. Den Übernahme-Dialog sieht nur ein **fremdes** Gerät.

## Akkustand

Wenn das Tablet seinen Akkustand meldet, sieht die Turnierleitung, wann ein
Gerät getauscht werden sollte. Dafür müssen aber Bedingungen erfüllt sein — die
Anzeige bleibt oft leer, und das ist **kein Fehler**:

- **iPads geben den Akkustand grundsätzlich nicht heraus.**
- **Android-Tablets nur unter Bedingungen:** über eine gewöhnliche
  unverschlüsselte Verbindung im Hallen-WLAN melden auch sie nichts. Es braucht
  entweder eine verschlüsselte Verbindung oder den Kiosk-Browser Fully, der den
  Wert selbst bereitstellt — siehe
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

**„Dieses Feld wird bereits geschiedst", obwohl niemand zählt.**
Meist hängt noch eine alte Sitzung eines anderen Geräts. **Court übernehmen**
löst das auf.

**Das Zahnrad-Menü lässt sich nicht öffnen.**
Es ist mit einer PIN geschützt, damit niemand aus Versehen das Feld wechselt.
Die PIN kennt die Turnierleitung; voreingestellt ist `0000`. Siehe
[Einstellungs-PIN & Kiosk-Sperre](tablet-kiosk.md).

**Die Pause läuft rot weiter.**
Das ist kein Fehler, sondern gewollt — sie wartet auf **Weiterspielen**.

**Das Tablet reagiert nach einem Punkt kurz nicht.**
Nach jedem gezählten Punkt ist die Zähltafel für einen Sekundenbruchteil
gesperrt. Das verhindert, dass ein hektischer Doppeltipp zwei Punkte vergibt.
Kurz warten und normal weitertippen — es ist nichts verloren.

**„Ergebnis übermitteln" bleibt gesperrt.**
Bei Aufgabe und kampfloser Wertung muss zuerst der **Sieger** gewählt werden.
Der Hinweis dazu steht direkt am Knopf.

**Ein Hinweis auf eine neue Fassung erscheint mitten im Spiel.**
Das Tablet lädt sich **nicht** von allein neu, solange gezählt wird. Den
Hinweis kannst du bis zum Spielende stehen lassen.

## Weiterlesen

- [Digitaler Tablet-Spielzettel](tablet.md) — wie der Betrieb aufgebaut ist
- [Schiri-Modus am Zähltablett](umpire-mode.md) — Karten und Ansagen
- [Einstellungs-PIN & Kiosk-Sperre](tablet-kiosk.md) — Gerät gegen Verstellen sichern
- [Punktverlauf-Graph](punktverlauf.md) — was der Verlauf zeigt
- [Zähltafelbediener einteilen](zaehltafelbediener.md) — wer wann ans Feld kommt
