# Master und Slave — wer steuert das Turnier

Wird in zwei oder mehr Hallen gleichzeitig gespielt, steht schnell mehr als ein
Rechner herum. Diese Seite beantwortet die Frage, die dann als Erstes kommt:
**Wer ist der Chef, und was macht der andere?**

Der technische Unterbau steht in [Mehr-Hallen-Turniere](multi-hall.md); hier
geht es um das Bild dahinter.

## Die Grundregel

> **Genau ein Rechner steuert das Turnier. Das ist der Master.**

Der Master ist der PC, auf dem der **Tournament Planner läuft** — oder der ihn
über das Netz erreicht. Nur er:

- liest und schreibt die Turnierdaten in BTP,
- vergibt Felder,
- schickt den Liveticker an badhub.de,
- druckt Schiedsrichterzettel.

Alles andere im Turnier hängt an ihm. Fällt er aus, steht das Turnier — deshalb
ist es der Rechner, den man nicht zum Musikabspielen benutzt.

**Ein Slave steuert nichts.** Er ist ein zweiter bts-light-Rechner in einer
weiteren Halle, der genau eine Aufgabe hat: **seine Halle ansagen**. Er schickt
nichts an den Liveticker, vergibt keine Felder, druckt nicht und schreibt
niemals etwas nach BTP.

Deshalb heißt der Schalter in den Einstellungen auch „Ansage-Slave-Modus" und
nicht etwa „zweiter Server".

## Brauche ich überhaupt einen zweiten Rechner?

Oft nicht. Ein Slave lohnt sich nur, wenn in der zweiten Halle **gesprochene
Aufrufe** gebraucht werden — die kommen aus den Lautsprechern des Rechners, der
dort steht, und lassen sich nicht sinnvoll aus der Nachbarhalle senden.

Was **ohne** zweiten Rechner funktioniert:

- Zähl-Tablets in der zweiten Halle,
- Court-Monitore und Info-Anzeigen dort,
- Ergebnisse, Liveticker, Feldvergabe.

Diese Geräte hängen nämlich **direkt am Master**, nicht am Slave — dazu unten
mehr. Wenn in Halle 2 also niemand etwas ansagen muss, reicht der eine PC.

## Die zwei Fälle

Welche Art Slave du brauchst, entscheidet eine einzige Frage: **Erreicht der
zweite Rechner den Tournament Planner über das Netz?**

### Fall 1 — Gleiches Netz: der LAN-Slave

Beide Hallen hängen im selben WLAN, der zweite Rechner kommt an die
BTP-Datenbank heran. Dann liest er sie **selbst** und sagt daraus seine Halle
an.

Einrichtung: **Ansage-Slave-Modus** einschalten, das Feld für den Master-Code
**leer lassen**, und auf der Seite **Ansagen** die eigene Halle wählen. Fertig.

### Fall 2 — Getrennte Netze: der Cloud-Slave

Die Hallen sind Kilometer auseinander oder hängen an getrennten Routern. Der
zweite Rechner kommt **nicht** an BTP heran — er hat schlicht keine
Turnierdaten.

Dann holt er sich das Nötige über badhub.de vom Master: welche Spiele auf
welchem Feld seiner Halle laufen und welche Freitexte anzusagen sind. Dafür
müssen die beiden **gekoppelt** werden.

## Koppeln in der Praxis

Der Weg ist auf ein Telefonat ausgelegt — jemand steht in der anderen Halle und
soll nichts abtippen, was 36 Zeichen lang ist.

**Am Master:** Einstellungen → **Telefon-Code erzeugen**. Es erscheint eine
**achtstellige Zahl**. Voraussetzung: Der Cloud-Weg ist aktiv und die
Übertragung läuft.

**In der fernen Halle:** Einstellungen → **Ansage-Slave-Modus** einschalten →
die acht Ziffern in das Feld tippen. Sobald die achte steht, löst die Software
den Code selbst ein und ersetzt ihn durch den langen Kopplungs-Code. Danach auf
der Seite **Ansagen** die eigene Halle wählen und speichern.

Zwei Dinge, die dabei erfahrungsgemäß Fragen aufwerfen:

- **Der Code ist eine Stunde gültig**, und ein neuer Code ersetzt den alten.
  Wer zu lange telefoniert, erzeugt einfach einen neuen.
- Der **lange Kopplungs-Code** steht darunter und lässt sich weiterhin
  kopieren und einfügen — der Telefon-Code ist nur die bequeme Abkürzung.

**Hat es geklappt?** Der Master zeigt in der Kopfzeile, ob die ferne Halle
verbunden ist, und blendet beim Verbinden kurz einen grünen Hinweis ein
(„Ferne Halle „X" hat sich verbunden ✓"). Bricht sie später weg, bleibt eine
**gelbe Warnung stehen**, bis sie wieder da ist. Das Wegbrechen bleibt also
nicht unbemerkt.

## Was in der fernen Halle woran hängt

Das ist der Punkt, an dem die meisten Missverständnisse entstehen:

| Gerät in der fernen Halle | Hängt an | Warum |
|---|---|---|
| Zähl-Tablets | **direkt am Master** (über badhub.de) | Das Ergebnis muss nach BTP, und BTP hat nur der Master. |
| Court-Monitore / TV-Anzeigen | **direkt am Master** | Sie zeigen den Spielstand, den der Master verteilt. |
| Ansagen | am **Slave** | Ton kommt aus den Lautsprechern vor Ort. |

Der Slave ist also **kein Zwischenhändler für Spieldaten**. Er sitzt daneben und
redet. Ein Ergebnis, das in Halle 2 auf einem Tablet eingetragen wird, geht
nicht über den Slave, sondern direkt zum Master und von dort nach BTP.

Damit die Crew in der fernen Halle ihre Feld-Adressen und QR-Codes nicht beim
Master erfragen muss, zeigt der Slave sie auf seiner eigenen Oberfläche an —
gefiltert auf seine Halle.

**Ausnahme für ältere Anzeige-Geräte:** Court-Monitor-Pis suchen ihren Server
im lokalen Netz und kennen keine Internet-Adresse. Damit sie auch in der fernen
Halle funktionieren, ohne dass jemand etwas umkonfiguriert, betreibt der Slave
dort eine kleine Brücke, die sie an den Master weiterreicht.

## Was nur der Master kann

**Schiedsrichterzettel drucken.** Ein Slave kennt die Besetzung eines frisch
belegten Feldes gar nicht — er hätte nichts zu drucken. Für die zweite Halle
richtet man deshalb dort einen **Netzwerkdrucker** ein und wählt ihn **am
Master** in den Einstellungen aus. Die Zettel gehen dann quer durchs Netz in
die richtige Halle.

**Punktverlauf und Zettel-Ablage** liegen aus demselben Grund immer beim
Master.

**Feldvergabe und Turnierleitung.** Wer in der fernen Halle etwas umdisponieren
will, tut das über die
[Turnierleitungs-Oberfläche im Browser](turnierleitung-web.md) — die läuft auf
einem Tablet oder Telefon und spricht mit dem Master. Nicht über den Slave.

## Wenn der Master umziehen muss

Der Master hat eine **Identität**, an der alle gekoppelten Geräte hängen:
Tablets, Monitore und die ferne Halle. Ein neuer PC bekäme eine neue Identität —
und alle Kopplungen wären still tot.

Deshalb gibt es unter **Wartung → Master-Identität umziehen** den Export in
eine Datei und den Import auf dem neuen Rechner. Danach laufen alle Geräte ohne
Neu-Koppeln weiter.

Drei Dinge dabei:

- **Der alte Master darf danach nicht mehr laufen.** Es darf immer nur einen
  geben.
- Nach dem Import muss bts-light **neu gestartet** werden.
- **Turnierleitungs-Geräte wandern nicht mit** und müssen am neuen PC neu
  gekoppelt werden.

Die Datei enthält keine Passwörter, aber den Kopplungs-Token — sie ist **wie
ein Passwort zu behandeln**.

## Häufige Missverständnisse

**„Der Slave übernimmt, wenn der Master ausfällt."** Nein. Ein Slave ist keine
Ausfallsicherung. Fällt der Master aus, steht das Turnier, bis er wieder läuft
oder seine Identität auf einen anderen Rechner umgezogen ist.

**„Jede Halle braucht ihren eigenen Rechner."** Nur, wenn dort angesagt werden
soll. Tablets und Anzeigen brauchen keinen.

**„Ich stelle in der zweiten Halle die Felder ein."** Der Slave steuert nichts.
Feldvergabe, Sperren und Reihenfolge laufen am Master oder über die
Turnierleitungs-Oberfläche.

**„Zwei Master gehen auch."** Nein — beide würden nach BTP schreiben und
einander überschreiben. Genau einer.

## Weiterlesen

- [Mehr-Hallen-Turniere](multi-hall.md) — der technische Unterbau
- [Cloud-Verbindung](cloud-relay.md) — wie die Verbindung über badhub.de
  funktioniert
- [Sprachansagen für Feld-Aufrufe](announcements.md) — Stimmen, Halle, Freitext
- [Einstellungen](einstellungen.md) — der Slave-Schalter und die Kopplung
