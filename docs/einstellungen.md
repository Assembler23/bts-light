# Einstellungen — was jede Option bewirkt

Diese Seite geht die Einstellungen der Windows-App der Reihe nach durch: was
eine Option tut, was voreingestellt ist, und ab wann eine Änderung wirkt.

Du erreichst sie über **Einstellungen** in der linken Leiste. Es ist dieselbe
Seite wie der Einrichtungs-Assistent beim ersten Start — nur heißt der Knopf
unten dann „Speichern" statt „Speichern & Liveticker starten".

## Zwei Dinge vorweg

**Jedes Speichern startet die Übertragung neu.** Der Knopf speichert nicht
nur, er hält die Verbindung zu BTP und badhub an und baut sie neu auf. Tablets,
Court-Monitore und Turnierleitungs-Geräte verbinden sich dabei neu — das dauert
wenige Sekunden und unterbricht nichts dauerhaft, aber **mitten im Spielbetrieb
merkt man es**. Wenn du nur ein Häkchen umlegen willst, ist ein ruhiger Moment
der bessere Zeitpunkt.

**Nicht alles wirkt sofort.** Manche Einstellungen liest die Software bei jeder
Anfrage frisch (Anzeigen, Turnierleitungs-Oberfläche, Tablets beim Neuladen),
andere erst beim Neustart der Übertragung. Da der Speichern-Knopf ohnehin neu
startet, ist das meist einerlei. Wo es einen Unterschied macht, steht es unten
dabei.

Der Speichern-Knopf bleibt **gesperrt**, solange etwas Wichtiges fehlt:

- keine BTP-Adresse,
- keine gültige Turnier-Kennung (im Slave-Modus entfällt diese Bedingung),
- weder LAN noch Cloud aktiv,
- oder — bei „Anderes Turnier (manuell)" — keine Badhub-URL beziehungsweise
  kein Badhub-Passwort. **Diese Bedingung gilt auch im Slave-Modus.**

## Ansage-Slave-Modus (zweite Halle)

Ganz oben, noch vor allem anderen. **Voreingestellt: aus.**

Ist der Schalter an, arbeitet dieser PC als **Slave** — als zweiter Rechner in
einer weiteren Halle. Er schickt dann **nichts** an den Liveticker, druckt keine
Schiedsrichterzettel und betreibt keinen eigenen Tablet-Server; er reicht seine
Halle an den Master durch. Entsprechend verlangt er keine Turnier-Kennung, und
mit einer Verbands-Kachel auch kein badhub-Passwort. (Im Modus „Anderes Turnier
(manuell)" bleiben Badhub-URL und -Passwort dagegen Pflicht.)

Genau ein Rechner im Turnier ist der Master. Mehr dazu:
[Mehr-Hallen-Turniere](multi-hall.md).

**Ferne Halle über Cloud einrichten** *(nur sichtbar, wenn der Slave-Modus an
ist)* — hier trägst du den Code des Masters ein. Zwei Formen sind möglich:

- Der **Telefon-Code**: acht Ziffern. Sobald die achte Ziffer steht, löst die
  Software ihn selbst ein und ersetzt ihn durch den langen Kopplungs-Code. Du
  siehst dabei „Telefon-Code wird eingelöst …" und dann eine Bestätigung.
- Der **lange Kopplungs-Code** direkt, wenn du ihn abtippen oder einfügen
  kannst.

Bleibt das Feld **leer**, arbeitet der PC als LAN-Slave und liest BTP selbst.

**Telefon-Code erzeugen** *(nur am Master sichtbar, also wenn der Slave-Modus
aus ist)* — erzeugt einen achtstelligen Code, den du der fernen Halle am
Telefon durchgeben kannst. Voraussetzung: Der Cloud-Weg ist aktiv und die
Übertragung läuft. Darunter steht zusätzlich der lange Kopplungs-Code mit einem
Knopf zum Kopieren.

## 1 · Liveticker-Ziel

**Verband wählen.** Sechs Kacheln (BVBB, BVRP, HBV, BBV, BWBV, NBV). Die Wahl
setzt Adresse und Zugangspasswort des Livetickers automatisch — du musst nichts
eintippen. Voreingestellt ist BVBB, sofern sich aus einer vorhandenen
Einrichtung nichts anderes ergibt.

Rechts an jeder Kachel sitzt ein Knopf **Code**. Er kopiert den Einzeiler für
die „Jetzt live"-Box in die Zwischenablage — den kann eine Vereins-Webseite
einbinden, um den Liveticker direkt anzuzeigen.

**Turnier bei turnier.de** — **Pflichtfeld** (außer im Slave-Modus). Du kannst
die komplette turnier.de-Adresse hineinkopieren; die Turnier-Kennung wird
selbst herausgezogen. Sieht die Eingabe nicht nach einer Kennung aus, warnt die
Seite. Ohne gültige Kennung **startet die Übertragung nicht** — die Meldung
lautet dann „Die Turnier-Kennung von turnier.de fehlt".

Diese Kennung hängt an mehr als am Liveticker: Der Hallen-Check-In sendet ohne
sie nichts, und Punktverlauf sowie abgelegte Schiedsrichterzettel werden damit
dem richtigen Turnier zugeordnet.

**Anderes Turnier (manuell)** — blendet den Abschnitt „3 · Badhub-Zugang" ein,
in dem du Adresse und Passwort selbst einträgst. Für Betreiber mit eigener
Instanz.

**Testsystem (test.badhub.de)** — ein Probelauf ohne Folgen. Liveticker,
Check-In, Cloud-Verbindung der Tablets und Diagnose-Logs gehen dann auf das
Testsystem, die Produktiv-Datenbank bleibt unberührt. Der Schalter erscheint
nur, wenn die eingestellte Adresse überhaupt auf badhub.de zeigt; bei einer
eigenen Instanz bleibt sie unverändert.

> Die **Cloud-Verbindung der Tablets** zieht erst beim Neustart der Übertragung
> nach — also mit dem Speichern.

**Liveticker-Passwort des Testsystems** *(nur bei aktivem Testsystem und
gewähltem Verband — im manuellen Modus steht das Passwort in Abschnitt 3)* — das
Testsystem hat eigene Zugangsdaten. Produktiv gilt immer das Passwort des
gewählten Verbands.

## Turnierlogo (optional)

Erscheint oben auf der Live-Seite, auf Schiedsrichterzetteln, auf dem Aushang
und auf den Anzeigen in der Halle. BTP liefert kein Logo — deshalb hier.

- **Logo wählen / ersetzen** — PNG, JPG, WEBP, GIF oder SVG, **höchstens 2 MB**.
  Andere Dateien und zu große Bilder werden mit einer Meldung abgewiesen.
- **Entfernen** *(nur wenn ein Logo hinterlegt ist)*.
- **Hintergrundfarbe** *(nur wenn ein Logo hinterlegt ist)* — für Logos, die
  auf Weiß schlecht stehen. Ohne eigene Farbe gilt der badhub-Standard.

Ein **neues Bild** wird zusätzlich einmalig an den Check-In übertragen; eine
reine Farbänderung nicht.

## 2 · BTP-Verbindung

| Feld | Voreinstellung | Bedeutung |
|---|---|---|
| **BTP-Adresse** | `127.0.0.1` | Der Rechner, auf dem der Tournament Planner läuft. `127.0.0.1` heißt „derselbe PC". |
| **Port** | `9901` | Der Netzwerk-Port des TP-Netzwerkdienstes. Leer oder unsinnig ⇒ es gilt wieder 9901. |
| **BTP-Passwort (falls gesetzt)** | leer | Nur nötig, wenn im Tournament Planner eines vergeben wurde. |

**Verbindung testen** prüft sofort und zeigt bei Erfolg den Turniernamen an —
das ist die schnellste Kontrolle, ob du beim richtigen Turnier bist. Der Test
**speichert nichts**; zum Übernehmen musst du unten trotzdem speichern.

## 3 · Badhub-Zugang

*Nur sichtbar, wenn oben „Anderes Turnier (manuell)" gewählt ist.*

**Badhub-URL** und **Badhub-Passwort** sind dann Pflicht. **Live-Seite (URL,
optional)** ist die öffentliche Adresse, die auf dem Aushang und in den
QR-Codes landet.

## Tablet-Verbindung

Wie erreichen die Zähl-Tablets diesen PC? **Beide Wege lassen sich zusammen
aktivieren** — mindestens einer muss an sein, sonst bleibt der Speichern-Knopf
gesperrt.

- **LAN – lokales Netz** *(voreingestellt an)*: Die Tablets verbinden sich
  direkt im Hallen-WLAN. Schnell und ohne Internet — braucht aber einen
  freigegebenen Port in der Windows-Firewall.
- **Über badhub.de – Cloud** *(voreingestellt aus)*: PC und Tablets verbinden
  sich nur nach außen. Funktioniert auch hinter gesperrten Firmen-Firewalls,
  setzt aber Internet voraus. Näheres: [Cloud-Verbindung](cloud-relay.md).

**Tablet-Einstellungs-PIN** — **voreingestellt `0000`**, nur Ziffern, höchstens
acht. Schützt das Zahnrad-Menü am Zähltablett davor, dass jemand aus Versehen
das Feld wechselt. Ein leeres Feld setzt die PIN auf `0000` zurück. Das ist ein
Bedien-Schutz, **keine Gerätesperre** — die macht der Kiosk-Browser, siehe
[Einstellungs-PIN & Kiosk-Sperre](tablet-kiosk.md).

## Ansagen

**Ansagen aktivieren** — **voreingestellt aus**. Hier gibt es bewusst nur den
einen Schalter; Stimme, Tempo, Gong, Aussprache und Halle stehen auf der
eigenen Seite **Ansagen** in der linken Leiste. Solange der Schalter aus ist,
ist dieser Menüpunkt ausgegraut. Details:
[Sprachansagen für Feld-Aufrufe](announcements.md).

Der Schalter wirkt **sofort**, ohne Neustart; eine laufende Ansage wird beim
Ausschalten abgebrochen.

## Aufruf-Timer

Hält fest, wie lange ein Feld schon gerufen ist, und stuft den Aufruf hoch.

| Feld | Voreinstellung | Bedeutung |
|---|---|---|
| **Aufruf-Timer aktivieren** | aus | Schaltet die Stufen 2. und 3. Aufruf ein. |
| **2. Aufruf nach (Minuten)** | 2 | Ab hier gilt der Aufruf als zweiter. |
| **3./letzter Aufruf nach (Minuten)** | 4 | Ab hier als dritter und letzter. |
| **Feld färbt sich rot, wenn nach (Minuten) noch kein Punkt gefallen ist** | 5 | Einfärbung in der **Turnierleitungs-Oberfläche**. |

Die beiden Aufruf-Zeiten sind nur sichtbar, wenn der Timer an ist. **Die rote
Einfärbung steht bewusst außerhalb** — sie wirkt auch ohne Aufruf-Timer.

Sie erscheint in der [Turnierleitungs-Oberfläche im Browser](turnierleitung-web.md),
nicht in der Spielübersicht der Windows-App. Dort bedeutet Rot etwas anderes,
nämlich **Feld gesperrt**.

Trägst du für den 3. Aufruf eine Zeit ein, die **nicht nach** dem 2. liegt,
warnt die Seite und hebt den Wert beim Speichern selbst an. Sobald der erste
Punkt gezählt ist, endet die Stufenzählung.

## Startzeit-Prognose

| Feld | Voreinstellung | Bedeutung |
|---|---|---|
| **Prognose anzeigen** | **an** | Zeigt geschätzte Startzeiten als „~hh:mm". |
| **Angenommene Spieldauer ohne Messwerte (Minuten)** | 25 | Nur wirksam, solange zu wenige echte Messwerte vorliegen. |

Sobald genügend Spiele gemessen sind, rechnet die Software mit den echten
Zeiten und der eingetragene Wert spielt keine Rolle mehr. Mehr dazu:
[Spielzeiten & Startzeit-Prognose](spielzeiten-prognose.md).

## Hallen-Check-In

| Feld | Voreinstellung | Bedeutung |
|---|---|---|
| **Hallen-Check-In aktivieren** | aus | Spieler bestätigen ihre Anwesenheit über eine Webseite. |
| **Namen in der „Es fehlen noch"-Ansage** | 8 | Wie viele Namen die Ansage vorliest. `0` = nie Namen, nur die Anzahl. |

**Wichtig:** Ohne gültige Turnier-Kennung aus Abschnitt 1 bleibt der Check-In
aus — auch wenn das Häkchen gesetzt ist. Solange er aus ist, ist der Menüpunkt
**Check-In** ausgegraut. Details: [Hallen-Check-In](spieler-check-in.md).

## Zähltafelbediener

**Zähltafelbediener-Warteschlange führen** — **voreingestellt aus**. Ist sie an,
kommt der Verlierer eines regulär beendeten Spiels in die Warteschlange.
**Walkover und Aufgabe erzeugen keinen Eintrag.** Details:
[Zähltafelbediener einteilen](zaehltafelbediener.md).

## Schiedsrichter

| Feld | Voreinstellung | Bedeutung |
|---|---|---|
| **Mit Schiedsrichtern spielen** | aus | Schaltet die Schiedsrichter-Verwaltung frei. |
| **Schiedsrichter automatisch rotieren** | aus | Nächster Einsatz wird selbst vorgeschlagen. |
| **Aufschlagrichter automatisch rotieren** | aus | Dasselbe für Aufschlagrichter. |

Die beiden Rotations-Schalter sind nur sichtbar, wenn der erste an ist. Bei der
Rotation werden übersprungen: wer pausiert, wer gerade im Einsatz ist, und wer
einen Vereins- oder Personenkonflikt mit dem Spiel hat.

Diese drei Häkchen wirken **sofort**, nicht erst beim nächsten Neustart.
Sperrlisten und Reihenfolge gehören zum Turnier, nicht zur Einrichtung, und
stehen deshalb nicht hier. Details:
[Schiedsrichter verwalten](schiedsrichter-management.md).

## Schiedsrichterzettel

| Feld | Voreinstellung | Bedeutung |
|---|---|---|
| **Zettel bei der Feldvergabe automatisch drucken** | aus | Druckt beim Belegen eines Feldes von allein. |
| **Drucker** | leer = Windows-Standarddrucker | Zieldrucker für den Zettel. |

> **Autodruck braucht Schiedsrichter.** Gedruckt wird nur für Spiele, denen ein
> **Schiedsrichter** zugeordnet ist. Ist oben „Mit Schiedsrichtern spielen"
> aus, druckt die Automatik nie. Ein Aufschlagrichter allein genügt nicht.

Die Drucker-Liste ist **zunächst leer** — sie füllt sich erst, wenn du
**Drucker suchen** drückst. Ein bereits eingestellter Drucker bleibt auch dann
wählbar, wenn Windows ihn gerade nicht meldet.

Gedruckt wird **A4 quer**, ohne Dialog, auf dem Rechner, auf dem BTS Light
läuft. Jedes Spiel wird **genau einmal** gedruckt; ein Neustart druckt nichts
nach. Details: [Schiedsrichterzettel drucken](schiedsrichterzettel.md).

## Automatische Feldvergabe

| Feld | Voreinstellung | Bedeutung |
|---|---|---|
| **Automatische Feldvergabe aktivieren** | aus | Freie Felder werden selbst belegt. |
| **Wartezeit, bis ein freies Feld belegt wird (Minuten)** | 1 | `0` = sofort. |
| **Pause nach Spielende je Spieler (Minuten)** | 0 | `0` = es gilt die Pausenregel aus BTP. |
| **Aktive Halle (Tages-Halle, leer = alle)** | leer | Beschränkt die Vergabe auf eine Halle. |

Die drei unteren Felder sind nur sichtbar, wenn die Automatik an ist.
**Gesperrte Felder werden nie automatisch belegt.**

Zwei Besonderheiten im Mehr-Hallen-Betrieb:

- **Ohne** gesetzte aktive Halle werden nur Spiele verteilt, die du für die
  jeweilige Halle ausdrücklich „in Vorbereitung" gerufen hast.
- **Mit** gesetzter aktiver Halle ist die automatische Hallen-Vorverteilung
  ausgeschlossen — beides zusammen geht nicht.

## Disziplinen je Halle

*Erscheint erst, wenn mindestens zwei Hallen erkannt wurden oder schon Regeln
bestehen. Die Hallen kommen aus BTP, also erst nach der Verbindung.*

Hier legst du fest, in welcher Halle eine Disziplin gespielt wird. Je Regel
wählst du einen Geltungsbereich und eine Halle:

- **Kategorie** („Alle HE", „Alle DD", …) gilt als Standard für die ganze
  Disziplin.
- **Eine einzelne Auslosung** sticht diesen Standard für genau diese Klasse.

Ohne Regel gibt es keine Einschränkung. Eine Zeile, in der die Halle noch auf
„Halle wählen …" steht, ist unvollständig und **verschwindet beim Speichern**.
Felder ohne Hallenangabe bleiben immer erlaubt.

## Vereine anzeigen

| Feld | Voreinstellung | Bedeutung |
|---|---|---|
| **Vereinsnamen anzeigen** | aus | Verein unter den Spielernamen. |
| **Vereinslogos anzeigen** | aus | Wappen neben den Namen. |

Beides gilt **turnierweit für alle Geräte**, nicht je Gerät. Die Tablets
übernehmen die Änderung beim nächsten Neuladen. Ist für einen Verein kein Logo
hinterlegt, bleibt die Zeile schlicht ohne Wappen.

## Court-Monitor

**Court-Monitor aktivieren** — **voreingestellt aus**. Der Schalter blendet
lediglich die Monitor-Adressen und die Menüpunkte **Monitore** und
**Siegerehrung** ein. Die Anzeige-Seiten selbst sind **immer** erreichbar; ein
bereits eingerichteter Monitor hört also nicht auf zu arbeiten, wenn der
Schalter aus ist.

Alles Folgende ist nur sichtbar, wenn der Schalter an ist.

### Werbebilder

- **Werbung im Leerlauf anzeigen** *(voreingestellt an)* — ist sie aus, zeigt
  ein freies Feld eine neutrale Leerlauf-Seite statt der Bilder.
- **Werbebild hinzufügen** — JPG, PNG, WEBP oder GIF, **höchstens 8 MB** je
  Bild. Mehrfachauswahl möglich.
- Je Bild: ein **Anzeigename** (höchstens 80 Zeichen; bleibt er leer, trägt das
  Bild keinen Namen — „Werbebild 1", „Werbebild 2" … ist nur der graue
  Platzhalter im Eingabefeld),
  ein Häkchen **Leiste** (kleine Sponsorenleiste neben dem Turnierlogo), eine
  **Hintergrundfarbe** und ein Häkchen **Feld zeigen** (die Feldbezeichnung
  steht dann auf dem Bild). Eine Vorschau zeigt sofort, wie die Beschriftung
  auf der gewählten Farbe wirkt.
- Das Papierkorb-Symbol entfernt ein Bild samt seiner Einstellungen.

> Diese Angaben werden **beim Klick beziehungsweise beim Verlassen des Feldes
> sofort gespeichert** — sie hängen nicht am Speichern-Knopf unten.

### Anzeige

- **Wechsel-Intervall** — Schieberegler von 3 bis 30 Sekunden,
  **voreingestellt 10**.
- **Layout** — derzeit nur „A — Geteilt (oben/unten)".
- Vier Häkchen, **alle voreingestellt an**: **Disziplin in der Kopfzeile**,
  **Runde in der Fußzeile**, **Spielnummer in der Fußzeile**, **Spieldauer in
  der Kopfzeile (Stoppuhr)**.
- **Pausen-Countdown (Retro-Klappanzeige)** *(voreingestellt an)* — erscheint
  am Monitor nur während einer Spielpause.

Die Vorschau daneben ändert sich mit den Häkchen. Anders als die Werbebilder
oben hängen Layout, Wechsel-Intervall und die Häkchen **am Speichern-Knopf** —
und greifen dann ohne weiteres Zutun, die Monitore müssen nicht neu geladen
werden. Details:
[Court-Monitor — TV-Anzeige am Spielfeld](court-monitor.md).

## Diagnose

**Diagnose-Logs automatisch an badhub senden.** Ist der Schalter an, wird etwa
alle zehn Minuten die aktuelle Logdatei übertragen. Sie enthält nur technische
Daten, **keine Spielernamen**. Das hilft, einen Fehler nach dem Turnier
nachzuvollziehen.

Der Startwert hängt davon ab, woher deine Einrichtung kommt:

| Ausgangslage | Schalter |
|---|---|
| **Neuinstallation** (seit v0.9.279) | **an** — der Assistent zeigt das Häkchen gesetzt |
| Bestehende Installation, die vor v0.9.279 eingerichtet wurde | **bleibt aus** — ein Update schaltet nichts stillschweigend ein |
| Du hast den Schalter selbst umgelegt | **deine Entscheidung gilt**, in beide Richtungen |

**Abwählen ist jederzeit möglich** und bleibt dauerhaft bestehen. Details:
[Diagnose-Logs](logging.md).

## Wartung

Eigener Menüpunkt, nicht Teil der Einstellungen.

**Updates & Logs**

- **Nach Update prüfen** — sucht sofort nach einer neuen Version. Beim
  Programmstart geschieht das ohnehin automatisch; die eigentliche Installation
  läuft über das Banner am oberen Rand.
- **Logs öffnen** — öffnet den Ordner mit den Logdateien im Explorer.
- Darunter steht die installierte Version.

**Master-Identität umziehen**

Damit ziehst du auf einen anderen PC um, ohne alle Tablets, Monitore und fernen
Hallen neu koppeln zu müssen.

- **Identität exportieren** legt eine Datei `bts-light-identitaet.json` an.
  **Passwörter sind darin nicht enthalten** — BTP-Passwort, badhub-Passwort und
  der Azure-Schlüssel werden entfernt. Trotzdem enthält sie den Kopplungs-Token
  und ist **wie ein Passwort zu behandeln**.
- **Identität importieren** übernimmt eine solche Datei nach einer
  ausdrücklichen Rückfrage. Vorhandene Passwörter auf diesem PC bleiben
  erhalten, wenn die Datei keine mitbringt.

Zwei Dinge sind dabei wichtig: **Der bisherige Master darf danach nicht mehr
laufen** — es darf immer nur einen geben. Und nach dem Import muss BTS Light
**neu gestartet** werden. **Turnierleitungs-Geräte wandern nicht mit** und
müssen am neuen PC neu gekoppelt werden.

## Was hier nicht eingestellt wird

| Wo | Was |
|---|---|
| Seite **Ansagen** | Stimme, Tempo, Gong, Aussprache-Korrekturen, Halle je Ansage |
| Seite **Turnierleitung** | Kopplung der Turnierleitungs-Geräte, Panel-Profile |
| Seite **Monitore** | Zuweisung der einzelnen Anzeigegeräte zu Feldern |
| **Feldübersicht** / Turnierleitungs-Oberfläche | Feld sperren, Feldvergabe von Hand, Spielreihenfolge |
| Tournament Planner | Auslosungen, Zeitplan, Meldungen — BTS Light ändert daran nichts |
