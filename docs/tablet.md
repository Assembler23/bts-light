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
   ein `tupdate_match` und pusht es an den Liveticker. `score_update` und
   `state_sync` tragen die **Match-ID** des gezählten Spiels: Passt sie
   nicht (mehr) zum aktuellen Match des Felds, verwirft der Server den
   Frame (**Stale-Filter, Cluster A4** — ein nach Doze/Reconnect im alten
   Spiel hängendes Tablet darf beim Neu-Zuweisen nicht den alten Stand
   unters neue Spiel schreiben; Turnier-Befund HM-03 19.07.2026). Alte
   Tablet-Seiten ohne das Feld laufen ungefiltert weiter.
3. **Endergebnis** – „Ergebnis übermitteln" → `POST /result` → bts-light
   meldet sich per LOGIN an und schreibt das Match mit `SENDUPDATE` zurück
   nach BTP (siehe [btp_protocol.md](btp_protocol.md)).

## Match-Setup (Seiten- und Aufschlagwahl)

Sobald ein Match aufs Feld kommt, führt ein kurzer Assistent durch die
Aufstellung:

1. **Seitenwahl** – welches Team steht links?
2. **Aufschlag** – wer schlägt zuerst auf?
3. **Annahme** (nur Doppel) – wer nimmt den Aufschlag an?

- **Aufschlag/Annahme nach jedem Satz neu** (Doppel/Mixed, seit v0.9.105):
  Aufschläger und Annehmer können je Satz wechseln. Endet ein Satz und das
  Match läuft weiter, fragt das Tablet nach der **Satzpause** erneut
  „**Neuer Satz — wer schlägt auf?**" — die Auswahl ist auf das
  **Gewinnerteam des letzten Satzes** beschränkt (BWF: der Satzgewinner
  schlägt zuerst auf), danach folgt die Annehmer-Wahl im Gegnerteam. Bis
  zur Bestätigung ist die Zähltafel gesperrt. **Einzel** braucht keine Wahl
  und läuft mit getauschter Aufstellung automatisch weiter. Die Wahl
  übersteht einen Tablet-Reload (`serveSetupTeam` wird persistiert);
  „Korrektur — letzter Punkt zurück" in der Satzpause hebt sie wieder auf.
- **Zurück-Schritt:** Ab Schritt 2 gibt es einen **„← Zurück · Back"**-
  Button. Er verwirft die zuletzt getroffene Wahl und springt einen
  Schritt zurück – so lassen sich Fehleingaben korrigieren, ohne das
  Match neu zuweisen zu müssen. (Bei der Per-Satz-Aufschlagwahl entfällt
  das Zurück: das Gewinnerteam steht durch den Satzstand fest.)
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
- **Kein Ton am Tablet (bewusst):** Das Tablet gibt **weder Gong noch
  Sprachansage** aus – es ist ein reiner Spielzettel am Feld. Gong und
  Ansage laufen ausschließlich auf den Ansage-Rechnern (Turnierleitung +
  ferne-Halle-Slave, `src/io/announcer.ts`), nie in `tablet.html`.

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

## „Match beenden" (Dialog: Aufgabe oder Kampflos)

Der dezente Button **„Match beenden …"** in der Fußzeile ist **ab 0:0**
verfügbar (vorher erst ab dem 2. Satz). Ein Tippen öffnet eine zweisprachige
Rückfrage (**„Spiel beenden? · End the match?"**) mit den Optionen:

- **Aufgabe – nur dieses Spiel · Retire (this match)** → Status **Aufgabe**
  (`ScoreStatus = 2`). Der laufende Teilstand wird als Satz übernommen. Es
  zählt **nur dieses Spiel**, keine Folgespiele.
- **Verletzung – auch Folgespiele der Disziplin · Injury** → wie Aufgabe, aber
  zusätzlich wird für die **restlichen Spiele der Disziplin** ein Walkover-
  Vorschlag hinterlegt (echte Verletzung, Spieler fällt aus). Siehe
  [walkover.md](walkover.md).
- **Kampflos · Walkover** → Status **Kampflos** (`ScoreStatus = 1`). Das Spiel
  wird **ohne Sätze** gewertet (z. B. Nichtantritt), die Satzliste wird verworfen.
- **Regulär beenden · Finish normally** → nur sichtbar, wenn schon Sätze
  gespielt wurden; beendet wie der frühere Button manuell anhand der Sätze.
- **Abbrechen · Cancel**.

Bei Aufgabe **und** Kampflos wählt der Schiedsrichter danach im Match-Ende-
Overlay den Sieger; erst dann lässt sich das Ergebnis übermitteln. Der Status
wird über `POST …/result` (Feld `retired` bzw. `walkover` + `winner`) an
bts-light gemeldet und per `SENDUPDATE` (`ScoreStatus`) nach BTP geschrieben
(LAN- und Cloud-Modus). Aufgabe und Kampflos schließen sich aus — der Server
weist beide gesetzten Flags ab.

## Ergebnis direkt eintragen (niemand hat gezählt)

Der ebenfalls dezente Button **„Ergebnis eintragen …"** in der Fußzeile
(offen sichtbar — ein Spieler muss ihn zur Not selbst bedienen können)
öffnet einen Dialog, in dem die **Satzstände** direkt eingetippt werden.
Anwendungsfall: Es hat niemand live am Tablet gezählt, das reguläre
Ergebnis soll trotzdem übermittelt werden.

- Die Spalten sind mit den Team-Namen der **linken/rechten** Court-Hälfte
  beschriftet; „+ Satz" ergänzt eine Zeile (bis zur Satzanzahl des
  Formats). Der aktuelle Stand ist vorbelegt, falls doch schon gezählt
  wurde.
- **Plausibilität clientseitig:** Jeder Satz muss regulär zu Ende gespielt
  sein (`setWinnerSide` gegen Ziel/Cap der BTP-Zählweise; im **Zeitformat**
  `target ≥ 99` genügt ein Satz, sobald er **nicht unentschieden** ist), es
  muss ein **eindeutiger Match-Sieger** herauskommen und es dürfen **keine
  überzähligen Sätze** dabei sein (der Sieg muss erst mit dem letzten Satz
  feststehen) — sonst erscheint eine Meldung im Dialog. Die Satzregel ist
  dieselbe wie serverseitig (`server::set_is_complete`, siehe
  [walkover.md](walkover.md)).
- „Übernehmen" füllt die Sätze, markiert das Match als beendet und öffnet
  das **normale Match-Ende-Overlay** (Sieger + „Ergebnis übermitteln") —
  ab da läuft alles über den bewährten, gegen Netzausfälle abgesicherten
  Sende-/Retry-Weg wie beim Live-Zählen. „Korrektur — Match wieder
  öffnen" macht die Eingabe rückgängig.

Für Kampflos/Aufgabe ist weiterhin der Dialog **„Match beenden"** da.

### Mitten im Spiel einsteigen und weiterzählen (Plan 12b)

Findet sich erst mitten im Spiel jemand zum Zählen, schaltet der Haken
**„Spiel läuft noch"** im selben Dialog den Übernahme-Modus ein:

- Oben die **abgeschlossenen Sätze**, darunter der **aktuelle Satz
  (läuft)** — beides wird plausibilisiert (abgeschlossene Sätze regulär
  zu Ende, das Match darf damit noch **nicht** entschieden sein; der
  laufende Satz darf **noch nicht** entschieden sein).
- „Weiterzählen" übernimmt den Stand und führt durch die gewohnte
  **Aufstellung** (Seitenwahl → Aufschläger → im Doppel Annehmer). Danach
  zählt das Tablet ab dem eingegebenen Stand normal weiter.
- **Aufschlagposition:** `finalizeSetup` platziert die Service-Courts
  regelkonform zum Stand — steht das aufschlagende Team auf einem
  **ungeraden** Punktestand, spielt es aus dem linken Service-Court
  (BWF-Parität, `computeServing`). Die Positionslogik ist durch
  `scripts/test-serving.mjs` (CI) abgesichert. Die Intervall-/Decider-Flags
  (`intervalDoneThisGame`, `midGameSwitchDone`) werden aus dem
  eingegebenen Stand abgeleitet, damit die 60-s-Pause bzw. der
  Entscheidungssatz-Seitenwechsel nicht doppelt kommt.

**Bekannte Feinheiten:** Liegt der eingegebene Stand **genau** auf der
Intervall-Schwelle (z. B. 11), gilt die 60-s-Pause als bereits erledigt
(Schutz gegen Doppel-Pause; im Grenzfall eine Annahme). Ein **Fehlgriff
in der Aufstellung** (falsche Seite/Aufschläger) lässt sich am einfachsten
korrigieren, indem man den kurzen Assistenten zu Ende führt und dann im
Match-Ende- bzw. über „Korrektur" neu ansetzt — ein Rückschritt aus der
Seitenwahl heraus gibt es (wie beim normalen Spielstart) nicht.

## Kampflose Wertung nach Aufgabe

**Nur auf ausdrückliche Wahl** (Dialog-Option „Verletzung – auch Folgespiele
der Disziplin") schlägt bts-light vor, die restlichen Spiele der aufgebenden
Mannschaft in derselben Disziplin kampflos (Walkover) für den jeweiligen Gegner
zu werten. Bei „Aufgabe – nur dieses Spiel" passiert das **nicht**. (Früher
kaskadierte jede Aufgabe automatisch.) Eigenes Feature-Dokument:
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

### Reconnect ist keine Übernahme (seit v0.9.147)

Jedes Tablet trägt eine **persistente Geräte-Kennung** (`deviceId`,
localStorage, einmalig erzeugt) und sendet sie bei `identify` und
`take_over` mit. Verliert ein Tablet kurz das Netz, hält seine tote
Verbindung das Feld serverseitig noch einige Sekunden — meldet sich
**dasselbe Gerät** zurück, löst es diese alte Session **nahtlos** ab:
kein „Feld belegt"-Overlay, kein manueller Übernehmen-Tap. Nur ein
**fremdes** Gerät sieht weiterhin den Übernehmen-Dialog. Alte
Tablet-Seiten ohne Kennung verhalten sich wie bisher.

Zusätzlich schützt eine **Stand-Revision** die offline gezählten Punkte:
Jede lokale Änderung zählt `rev` im persistierten Snapshot hoch. Schickt
der Server beim Reconnect seinen (während des Aussetzers veralteten)
Spielstand (`state_restore`), gilt **„neuer gewinnt"**: Hat das Tablet
zum selben Match einen gleich neuen oder neueren Stand, behält es ihn
und spiegelt ihn sofort zurück (Server-Cache + Liveticker ziehen nach) —
vorher überbügelte der alte Server-Stand die weitergezählten Punkte
(Turnier-Befund 18.07.2026). Ein frisches Gerät (Reload ohne Stand,
Ersatz-Tablet, echte Übernahme) übernimmt den Server-Stand unverändert.

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
- **Bildschirm-Schlaf am Tablet ausschalten** (Keep Screen On — in
  Fully Kiosk die Option „Bildschirm an lassen", sonst in den
  Android-/iPad-Display-Einstellungen den Timeout auf „nie" stellen).
  Turnier-Log 19.07.2026: **140** Doze-Reconnect-Zyklen an einem Tag —
  funktional folgenlos (der Reconnect heilt sich seit v0.9.147 selbst),
  aber jede Doze-Phase macht die Anzeige träge und flutet das Log.
  Ein programmatischer Wake Lock braucht HTTPS (Secure Context) und
  kommt mit ADR 0005 (LAN-HTTPS).
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
