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

Klappt das Scannen nicht, steht die Adresse unter dem QR-Code auch als Text.
Sie enthält den Zugang hinter einem `#` — dieser Teil wird von keinem
Browser an einen Server geschickt und taucht deshalb in keinem Protokoll auf.

**Vor dem Koppeln die Übertragung starten.** Läuft sie nicht, geht die
Adresse ins Leere, das Gerät übernimmt den Zugang nie — und der war nur
dieses eine Mal zu sehen. Die Seite warnt in diesem Fall.

## Im Betrieb

- **Zuweisen:** Spiel antippen, dann Feld antippen. Oder ziehen. Beides
  führt zum selben Ergebnis; bei abgebrochenem Ziehen bleibt die Auswahl
  stehen.
- **Umhängen:** Spiel auf dem Feld antippen, dann das Zielfeld. Das ist
  **ein** Schreibvorgang nach BTP — es gibt keinen Moment, in dem das Spiel
  auf keinem Feld steht.
- **Vom Feld nehmen:** über das Band oben. Läuft schon ein Spielstand, wird
  vorher gefragt.
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
- **Die Felder bleiben stehen**, während die Spielliste läuft: Auch ein
  Spiel von ganz unten lässt sich noch auf ein Feld ziehen oder tippen.
- **Ergebnis eintragen** für ein Spiel auf dem Feld.
- **Automatik** an- und abschalten — der Schalter oben rechts.

Gesprochen wird nie auf diesem Gerät: Die Seite beauftragt die Ansage, und
gesprochen wird sie dort, wo die Anlage hängt. Hört in der Zielhalle gerade
kein Ansage-Gerät zu, sagt die Seite das ausdrücklich.

## Grenzen, die im Betrieb auffallen

- **Acht Geräte gleichzeitig.** Das neunte wird abgewiesen; ein
  geschlossener Tab gibt seinen Platz nach einer Minute selbst frei. Die
  Liste der Kopplungen darf länger sein — alte Kopplungen blockieren keinen
  Platz.
- **Ergebnisse korrigieren geht nur, solange nichts daran hängt.** Ein
  bereits gewertetes Spiel lässt sich überschreiben, wenn kein Folgespiel
  existiert (Finale, Gruppenspiel). Sobald der Sieger im nächsten Spiel
  steht, wird abgelehnt — was BTP beim Überschreiben mit dem Turnierbaum
  macht, ist noch nicht abschließend geklärt
  ([btp_protocol.md](btp_protocol.md)). Bis dahin: in BTP von Hand.
- **Zähltafelbediener-Warteschlange** lässt sich hier nur ansehen, nicht
  umsortieren.
- **Beendete Spiele** zeigt die Seite nicht.
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
