# Digitaler Tablet-Spielzettel

Schiedsrichter zählen am Tablet statt auf Papier. bts-light betreibt dafür
einen eingebetteten Server, an den sich die Tablets im Hallen-WLAN hängen.
Am Spielende wird das Ergebnis nach BTP zurückgeschrieben.

## Architektur

bts-light ist der zentrale Hub – wie der Server der Original-BTS-Software.

```
Tablet Court 1 ─┐
Tablet Court 2 ─┼─ WS/HTTP ─▶ bts-light ──┬─▶ BTP    (SENDUPDATE bei Spielende)
Tablet Court 3 ─┘          (axum :8088)   ├─▶ badhub Liveticker (live)
                                          └─▶ Felder-Übersicht im bts-light-Fenster
```

- **Eingebetteter Server** – `axum` auf `0.0.0.0:8088`, läuft mit der
  Liveticker-Sync-Schleife (startet/stoppt mit „Starten"/„Stoppen").
- **Court → Match automatisch** – das Tablet ist an einen BTP-Court-Namen
  gebunden und zeigt das Spiel, das BTP gerade auf diesem Court hat. Keine
  manuelle Zuweisung – BTP ist die Quelle.
- **Score-Quelle pro Court** – zählt an einem Court ein Tablet, treibt es
  den Live-Score; sonst weiter das BTP-Polling. So überschreibt der
  5-Sekunden-Poll nie den Tablet-Stand.

## Verbindungsart: LAN oder Cloud

Die Tablets erreichen bts-light auf zwei Wegen – umschaltbar im
Setup-Wizard unter „Tablet-Verbindung":

- **LAN** – der hier beschriebene eingebettete Server. Schnell und
  offline, braucht aber den freigegebenen eingehenden Port 8088.
- **Cloud** – über einen Relay auf badhub.de; funktioniert auch hinter
  gesperrten Firmen-Firewalls (nur ausgehende Verbindungen). Details:
  [cloud-relay.md](cloud-relay.md).

LAN und Cloud sind zwei einzeln schaltbare Kacheln – **beide zusammen**
sind erlaubt: Bei einem Zwei-Hallen-Turnier bindet die Haupthalle ihre
Tablets per LAN an, eine zweite Halle übers Cloud-Relay. Bei diesem
Doppelbetrieb zeigt der Spielzettel je Feld beide QR-Codes (je einer pro
Weg); ein Tablet wählt seinen Weg über den gescannten QR-Code.

Dieses Dokument beschreibt den LAN-Modus. Im Cloud-Modus sind Daten- und
BTP-Schreibweg identisch – nur die Strecke Tablet ↔ bts-light läuft über
den Relay statt direkt.

## Endpunkte des Tablet-Servers

| Route | Zweck |
|---|---|
| `GET /` | Landing-Page mit allen Court-Adressen |
| `GET /court/{name}` | Tablet-Spielzettel-UI für einen Court |
| `GET /qr/{name}` | QR-Code (SVG) zur Court-URL |
| `GET /ws` | WebSocket (Match-Zuweisung, Live-Score) |
| `POST /result` | Endergebnis vom Tablet → `SENDUPDATE` nach BTP |
| `GET /health` | Status-Schnappschuss |

## Datenfluss

1. **Match-Zuweisung** – der Server prüft alle 2 s `match_for_court` und
   schickt dem Tablet `match_assigned` / `match_cleared`.
2. **Live-Score** – jeder Punkt am Tablet → `score_update` → bts-light baut
   ein `tupdate_match` und pusht es an den Liveticker.
3. **Endergebnis** – „Ergebnis übermitteln" → `POST /result` → bts-light
   meldet sich per LOGIN an und schreibt das Match mit `SENDUPDATE` zurück
   nach BTP (siehe [btp_protocol.md](btp_protocol.md)).

## Match-Setup (Seiten- und Aufschlagwahl)

Sobald ein Match aufs Feld kommt, führt ein kurzer Assistent durch die
Aufstellung:

1. **Seitenwahl** – welches Team steht links?
2. **Aufschlag** – wer schlägt zuerst auf?
3. **Annahme** (nur Doppel) – wer nimmt den Aufschlag an?

- **Zurück-Schritt:** Ab Schritt 2 gibt es einen **„← Zurück · Back"**-
  Button. Er verwirft die zuletzt getroffene Wahl und springt einen
  Schritt zurück – so lassen sich Fehleingaben korrigieren, ohne das
  Match neu zuweisen zu müssen.
- **Zweisprachig:** Titel und Hinweise des Assistenten erscheinen
  Deutsch **und** Englisch (internationale Spieler:innen). Das gilt auch
  für das Megafon-Popup.

## Am Tablet: Pausen, Court-Grafik, Akkustand

- **Offizielle Pausen** (BWF): Bei 11 Punkten im Satz blendet das Tablet
  eine 60-Sekunden-Pause ein, zwischen den Sätzen eine 2-Minuten-Pause –
  je mit Countdown. „Weiterspielen" beendet die Pause früher; bei 0 geht
  es automatisch weiter. Während der Pause ist die Zähltafel gesperrt.
- **Spieldauer**: läuft als MM:SS in der Kopfzeile ab Matchstart.
- **Court-Grafik**: zeigt Aufschläger (gelb markiert) und Annehmer auf
  dem Spielfeld – im Einzel ein Name je Hälfte, im Doppel zwei.
- **Akkustand**: Android-Tablets (Chrome) melden ihren Akkustand an die
  Felder-Übersicht in bts-light – so sieht die Turnierleitung, wenn ein
  Tablet getauscht werden sollte. iPads/Safari geben den Akkustand aus
  Datenschutzgründen nicht her; dort bleibt die Anzeige leer.

## Meldungen an die Turnierleitung

In der Kopfzeile rechts gibt es zwei Melde-Buttons:

- **✚ Verletzung/Behandlung** – unterbricht das Spiel (Behandlungspause
  ohne Countdown, „Weiterspielen" hebt sie auf). Das Feld wird in der
  bts-light-Felder-Übersicht rot hervorgehoben. In der Behandlungspause
  gibt es zusätzlich **„Spiel abbrechen"** (siehe unten).
- **📣 Turnierleitung rufen** – Popup (deutsch/englisch) mit Bestätigung;
  meldet, dass ein Offizieller ans Feld soll.

Beide Meldungen erscheinen zusätzlich in einer **app-weiten Leiste** in
bts-light – auf jeder Seite, mit Feldnummer. Aufgelöst werden sie am
Tablet (Behandlung „Weiterspielen" bzw. Meldung zurücknehmen).

## Spiel abbrechen (Aufgabe)

Gibt ein:e Spieler:in verletzungsbedingt auf, beendet **„Spiel abbrechen"**
in der Behandlungspause das Match. Der laufende Satz wird als Teilstand
übernommen (z. B. 21:10, dann 5:5), danach wählt der Schiedsrichter im
Match-Ende-Overlay manuell den Sieger. Das Ergebnis geht mit dem Status
**Aufgabe** (`ScoreStatus = 2`, „retired") nach BTP.

## Kampflose Wertung nach Aufgabe

Nach einer Aufgabe schlägt bts-light vor, die restlichen Spiele der
aufgebenden Mannschaft in derselben Disziplin kampflos (Walkover) für den
jeweiligen Gegner zu werten. Eigenes Feature-Dokument:
[walkover.md](walkover.md).

## Spiele in Vorbereitung aufrufen

Der Tablet-Spielzettel hat einen Tab **„In Vorbereitung"**: Die
Turnierleitung wählt dort eingeplante Spiele (feststehende Paarung, noch
nicht auf einem Feld) aus und ruft sie „in die Vorbereitung". Bei einem
Mehr-Hallen-Turnier lässt sich je Aufruf die Halle wählen, sodass die
Spieler rechtzeitig in die richtige Halle gehen.

Ein aufgerufenes Spiel wird im Liveticker-Push mit einem Zeitstempel
markiert und erscheint auf der Aufruf-Anzeige (`/live?display=next`)
hervorgehoben („vor X Min aufgerufen"). Der Aufruf lässt sich
zurücknehmen; kommt das Spiel auf ein Feld, verschwindet er automatisch.
BTP kennt keinen Vorbereitungs-Zustand – bts-light verwaltet ihn selbst.

## Tablet-Übernahme

Pro Court schiedst genau **ein** Tablet aktiv. Öffnet ein zweites Gerät
denselben Court, zeigt es „Dieses Feld wird bereits geschiedst" mit einem
**Übernehmen**-Button – gedacht für den Geräte-Tausch, etwa wenn ein
Tablet ausfällt. Das übernehmende Gerät setzt das **laufende Spiel mit
aktuellem Stand** fort (das aktive Tablet spiegelt seinen Stand dafür
laufend an den Server). Nach der Übernahme ist das alte Gerät gesperrt.

## Einrichtung im Turnier

1. In bts-light den Liveticker starten (BTP muss verbunden sein) – der
   Tablet-Server startet automatisch mit.
2. „Tablet-Spielzettel" öffnen → pro Court QR-Code/Adresse.
3. Am Spielfeld das Tablet mit dem Hallen-WLAN verbinden, die Court-URL
   öffnen (oder QR scannen).
4. BTP muss das Spiel dem Court zugewiesen haben – dann erscheint es
   automatisch auf dem Tablet.

## Voraussetzungen

- Tablet und bts-light-PC im **selben WLAN**.
- **Windows-Firewall**: beim ersten Start fragt Windows, ob der Zugriff
  erlaubt werden soll – „Zugriff zulassen" (private Netze). Ohne Freigabe
  erreichen die Tablets bts-light nicht. Auf gesperrten Turnier-PCs ohne
  Admin-Rechte hilft stattdessen der Cloud-Modus ([cloud-relay.md](cloud-relay.md)).
- In **BTP müssen Netzwerk-Edits erlaubt** sein, sonst lehnt BTP den
  `SENDUPDATE` ab – das Tablet zeigt dann einen Fehler.

## Fehlersuche

- bts-light läuft pro Rechner **nur einmal** (Single-Instance). Ein
  zweiter Start würde sonst den Tablet-Server-Port 8088 blockieren.
- Der Server protokolliert ausgelieferte Tablet-Seiten, Tablet
  verbunden/getrennt und Match-Zuweisungen ins Log – siehe
  [logging.md](logging.md). Steht dort kein „Tablet verbunden", erreicht
  das Tablet den Server nicht (WLAN/Firewall prüfen).

## Bekannte Vereinfachungen

- Spielsystem ist fest **Best-of-3 bis 21** (BTP liefert das Format nicht
  zuverlässig im aktuellen Parser).
- Liga-Matches (`PlayerMatches`) sind noch nicht abgedeckt.
