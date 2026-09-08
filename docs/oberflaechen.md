# Welcher Bildschirm zeigt was

BTS Light hat mehr Oberflächen, als man auf den ersten Blick vermutet: das
Programm auf dem Turnier-PC, die Seiten auf den Geräten in der Halle und die
öffentlichen Seiten auf badhub.de. Diese Übersicht sagt, **wo du was findest**
und **welches Gerät welche Seite anzeigt**.

Es sind drei Welten:

| Welt | Wer bedient sie | Wo |
|---|---|---|
| **Die Windows-App** | Turnierleitung | am Turnier-PC |
| **Die Seiten in der Halle** | Schiedsrichter, Hallenaufbau | Tablets, Fernseher, Raspberry Pi |
| **Die öffentlichen Seiten** | Spieler und Zuschauer | eigenes Telefon, Hallen-TV |

## Die Windows-App

Die linke Leiste, von oben nach unten. Ein Menüpunkt kann **ausgegraut** sein,
solange die zugehörige Funktion abgeschaltet ist — ein Klick darauf springt
dann in die Einstellungen an die richtige Stelle, statt eine leere Seite zu
zeigen.

| Menüpunkt | Wofür | Ausgegraut, wenn |
|---|---|---|
| **Status** | Die Startseite: Läuft die Übertragung? Wie weit ist das Turnier? Sind alle Felder mit Geräten versorgt? Von hier aus lassen sich die Anzeigen im Browser öffnen und der Aushang drucken. | nie |
| **Spielübersicht** | Das Arbeitspferd der Feldvergabe: oben die spielbereiten Spiele, darunter die Felder mit Ampel — grün frei, gelb belegt, **rot gesperrt**. Spiele zuweisen, Felder freigeben und sperren. | nie |
| **Tablets** | Drei Reiter: **Übersicht** (welches Tablet hängt an welchem Feld, mit Akkustand), **In Vorbereitung** (gerufene Spiele), **QR-Codes** zum Einrichten der Geräte. | nie |
| **Turnierleitung** | Hier entstehen die Zugänge für die Turnierleitungs-Oberfläche: Gerät koppeln, QR-Code zeigen, Zugang wieder entziehen. | nie — hier schaltet man sie schließlich ein |
| **Check-In** | Wer ist da, wer fehlt — je Klasse. Von Hand einchecken oder zurücksetzen, Fehlende ausrufen lassen, Check-In-Seite und QR-Aushang öffnen. | Check-In aus **oder** Turnier-Kennung fehlt |
| **Schiedsrichter** | Einsatzplanung: Reihenfolge, Pausen, Sperrlisten, feldweise Schalter. Die Stammliste kommt aus BTP. | „Mit Schiedsrichtern spielen" aus |
| **Ansagen** | Feld von Hand ansagen, Freitext sprechen, gespeicherte Ansagen, Verlauf mit „Erneut abspielen" — und alle Detail-Einstellungen zu Stimme und Tempo. | Ansagen aus |
| **Monitore** | Die Geräteverwaltung für Fernseher und Pis: **welches Gerät zeigt was**. Siehe unten. | Court-Monitor aus |
| **Siegerehrung** | Steuert, welche Disziplin der Sieger-Monitor gerade zeigt. Bewusst ohne Rotation, damit das Publikum das Podium fotografieren kann. | Court-Monitor aus |
| **Einstellungen** | Derselbe Bildschirm wie der Einrichtungs-Assistent. Siehe [Einstellungen](einstellungen.md). | nie |
| **Wartung** | Update prüfen, Logs öffnen, Version ablesen, Identität umziehen. | nie |

Über allem liegt eine **Kopfzeile** mit dem Start-/Stopp-Knopf und vier
Statusanzeigen:

- **Liveticker** — läuft die Übertragung gerade, oder ist sie gestoppt?
- **Hallennetz** — hängt der PC im WLAN der Halle?
- **Internet** beziehungsweise Cloud-Verbindung
- **Ferne Halle** — nur, wenn eine gekoppelt ist

## Die Geräte in der Halle

Alle diese Seiten kommen vom Turnier-PC. Die Adresse beginnt mit
`http://bts-light.local:8088` oder der IP-Adresse des PCs im Hallennetz.

### Zum Zählen

| Seite | Gerät | Was drauf ist |
|---|---|---|
| **Felder-Lobby** `/felder` | Zähl-Tablet | Die Startseite eines Tablets: eine Kachel je Feld mit Paarung und Zustand. Antippen führt zum Feld. |
| **Spielzettel** `/court/<Feld>` | Zähl-Tablet | Der eigentliche Zählbildschirm. Siehe [Das Zähltablett bedienen](tablet-bedienen.md). |
| **Zähltafel** `/court/<Feld>/tafel` | Tablet am Netz oder TV | Nur zwei große Punktzahlen und der Satzstand — wie eine klassische Tafel, ohne Namen. |
| **Anzeige-Hülle** `/anzeige` | Tablet, das nur anzeigen soll | Zeigt eine der Anzeigen bildschirmfüllend, mit Zahnrad zum Umschalten. Praktisch, wenn ein Tablet nicht zählt, sondern nur zeigen soll. |

> **Steht das Tablet hinter dem Feld?** Zähltafel und Spielzettel können die
> Punkte **übereinander** statt nebeneinander anordnen — vorn und hinten statt
> links und rechts. Von allein richtet sich das nach der Geräteausrichtung:
> Hochformat ergibt übereinander.
>
> Fest einstellen lässt es sich an zwei Stellen: beim **Spielzettel** im
> Zahnrad-Menü, bei der **Zähltafel** in der Anzeige-Hülle (dort erscheint die
> Auswahl, sobald als Ziel „Zähltafel" gewählt ist). Ein fest zugewiesener
> Fernseher richtet sich **immer** nach seiner Ausrichtung — dort gibt es keine
> feste Einstellung.

### Zum Anzeigen

| Seite | Gerät | Was drauf ist |
|---|---|---|
| **Court-Monitor** `/court/<Feld>/display` | TV am Spielfeld | Namen, Sätze, Punkte, Aufschlag, Aufruf-Uhr. Läuft kein Spiel: Werbung oder Leerlaufbild. |
| **Kombi-Anzeige** `/combo` | ein TV für zwei bis drei Felder | Mehrere Felder als Bänder auf einem Bildschirm. |
| **Hallen-Übersicht** `/info/overview` | großer TV oder Beamer | **Alle** Felder — auch die freien. Bei mehreren Hallen zeigt der Bildschirm eine Halle nach der anderen im Vollbild, statt alle zusammenzuquetschen. Ein Zusatz in der Adresse bindet ihn fest an eine Halle. |
| **In Vorbereitung** `/info/preparation` | TV am Meeting Point | Die als Nächstes anstehenden Spiele, gerufene zuerst. |
| **Siegerehrung** `/info/winners` | TV am Podest | Das Podium der gewählten Disziplin. Ein Bildschirm kann auch **nur einen Platz** zeigen — gedacht für je einen Monitor vor jeder Podeststufe. **Achtung beim dritten Platz:** Im Badminton gibt es **zwei** Dritte, und der Platz-3-Bildschirm zeigt beide untereinander — im Doppel also bis zu vier Namen. |
| **Werbung** `/info/ad` | Werbe-TV | Sponsorenbilder, rotierend oder als Einzelbild. |

### Zum Steuern

| Seite | Gerät | Was drauf ist |
|---|---|---|
| **Turnierleitungs-Oberfläche** `/tl` | Tablet oder Telefon | Dieselben Entscheidungen wie am PC, aber unterwegs in der Halle: Felder, Aufgaben, Zähltafel-Warteschlange, Schiedsrichter, Spiele, beendete Spiele, Spielzeiten, Anfangszeiten. Welche dieser Panels ein Gerät zeigt, lässt sich je Gerät einstellen. Siehe [Turnierleitungs-Oberfläche im Browser](turnierleitung-web.md). |

## Wie ein Gerät zu seiner Anzeige kommt

Ein frisch aufgestellter Fernseher zeigt **nicht** sofort ein Feld, sondern
nur das **Logo** — er wartet auf seine Zuweisung. Er meldet sich dabei von
selbst in der App: unter **Monitore** taucht er in der Liste auf und bekommt
dort sein Ziel:

- ein **Feld** (Court-Monitor),
- eine **Zähltafel** für ein Feld,
- eine **Information**: Hallen-Übersicht, In Vorbereitung, Siegerehrung,
- **Werbung**, rotierend oder ein bestimmtes Bild,
- eine **Kombi-Anzeige** aus mehreren Feldern.

Erst danach schaltet das Gerät selbständig auf die richtige Seite um. Du musst
an ihm nichts eintippen — das ist der ganze Sinn der Zuweisung.

**Welches Gerät ist welches?** Bei mehreren gleich aussehenden Fernsehern hilft
die Funktion **Identifizieren**: Der angeklickte Bildschirm blendet daraufhin
seinen Gerätecode ein, sodass du ihn in der Liste sicher zuordnen kannst.

## Die öffentlichen Seiten auf badhub.de

Diese Seiten gehören nicht zu BTS Light, sondern zu badhub — BTS Light füttert
sie nur. Alle erreicht man ohne Zugangsdaten.

| Seite | Wer schaut drauf |
|---|---|
| **Liveticker** | Zuschauer, Spieler, Eltern zu Hause: alle Felder Punkt für Punkt. |
| **Hallen-Monitor** | Die Online-Fassung der Court-Übersicht, für einen Hallen-TV mit Internet. Lässt sich fest an eine Halle binden. |
| **Nächste Spiele** | Die Online-Fassung der Vorbereitungs-Anzeige. |
| **Check-In-Seite** | Spieler auf dem eigenen Telefon beim Ankommen. |
| **Teilnehmerliste → eigene Spielerseite** | Ein Spieler sieht, in welcher Halle er spielt, wie viele Spiele noch vor ihm liegen und wann er etwa dran ist. |

Am schnellsten kommst du an diese Adressen über den **Aushang**: Ein A4-Blatt
mit zwei QR-Codes — Teilnehmerliste und Liveticker —, gedruckt über
**Status → Aushang drucken**. Siehe [Aushang für die Halle](aushang.md).

Aus der App heraus:

| Seite | Wo der Knopf sitzt |
|---|---|
| Liveticker, Hallen-Monitor, Nächste Spiele | **Status → Anzeigen im Browser öffnen** |
| Check-In-Seite | **Check-In → Check-In-Seite öffnen** |
| Teilnehmerliste | kein Knopf — sie steht als QR auf dem Aushang |

> Der Block „Anzeigen im Browser öffnen" erscheint nur, wenn eine öffentliche
> Live-Adresse eingetragen ist, und die drei Knöpfe sind grau, solange der
> Liveticker nicht läuft.

## Bildschirme, die man leicht übersieht

- **Der TV-Starter** `/` oder `/tv` — ein Kachel-Menü, das sich mit den
  Pfeiltasten einer Fernbedienung bedienen lässt. Zum schnellen Durchprobieren,
  welche Anzeige auf einen Fernseher soll. Dazu die Kurzadressen `/alle`
  (Hallen-Übersicht), `/next` (nächste Spiele) und `/h/1`, `/h/2` für die
  einzelnen Hallen — kurz genug, um sie an einer Fernbedienung einzutippen.
- **Die Statusseite** `/status` — eine schlichte Liste aller Felder mit ihren
  Adressen und QR-Links. Steht in keinem Menü, ist aber beim Aufbau der
  schnellste Weg.
- **Die Felder-Lobby** `/felder` — wer nur QR-Codes kennt, übersieht sie.
- **Die Anzeige-Hülle** `/anzeige` — erreichbar über das Zahnrad-Menü des
  Zähl-Tabletts und über „Nur Spielstand anzeigen", wenn ein Feld schon
  besetzt ist.
- **Die Android-Kiosk-App** — sie sucht den Turnier-PC selbst und öffnet die
  Felder-Lobby im Vollbild. Siehe
  [Zähl-Tablet als Kiosk-App (Android)](tablet-android-app.md).
- **Das Geräte-Panel des Slaves** — läuft dieser PC als Slave einer fernen
  Halle, zeigt sein Status-Bildschirm die QR-Codes und Monitor-Links **seiner**
  Halle. Die Crew dort muss nicht auf den Master-Bildschirm schauen. Siehe
  [Master und Slave](master-slave.md).

## Welcher Bildschirm für welche Rolle

**Turnierleitung.** Am PC ist die **Spielübersicht** das Arbeitspferd,
**Status** die Kontrollanzeige. Unterwegs in der Halle die
**Turnierleitungs-Oberfläche** auf Tablet oder Telefon — dieselben
Entscheidungen, ohne am PC zu stehen.

**Am Zähltablett.** Nur zwei Seiten: **Felder-Lobby**, dann der
**Spielzettel**. Alles Weitere passiert dort.

**Hallenaufbau.** **Monitore** in der App ist die zentrale Seite — dort bekommt
jedes Gerät sein Ziel. Am Gerät selbst hilft `/status` für die Adressen und
**Tablets → QR-Codes** für die Feld-Codes.

**Spieler.** Sehen von BTS Light selbst nichts. Für sie zählen die
**öffentlichen Seiten** und in der Halle die **Vorbereitungs-Anzeige** und die
**Hallen-Übersicht**.

## Weiterlesen

- [Installation und erste Schritte](erste-schritte.md)
- [Einstellungen — was jede Option bewirkt](einstellungen.md)
- [Court-Monitor — TV-Anzeige am Spielfeld](court-monitor.md)
- [Turnierleitungs-Oberfläche im Browser](turnierleitung-web.md)
