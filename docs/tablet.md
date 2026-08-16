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
| `GET /tl` | Turnierleitungs-Oberfläche (Seite) |
| `GET /tl/api/state` | Anzeige-Zustand der Turnierleitungs-Oberfläche |
| `POST /tl/api/command` | Aktion eines Turnierleitungs-Geräts |

Die `/tl/`-Routen gehören zur [Turnierleitungs-Oberfläche](features/turnierleitung-web.md).
Die **Schnittstellen-Routen** verlangen einen Zugang im `Authorization`-Kopf
(`Bearer …`) — bewusst im Kopf und nicht im Pfad, weil Pfade in
Zugriffsprotokollen landen. Ohne freigeschaltetes Feature
(`tl_web.enabled`, Default aus) und ohne bekanntes Gerät antworten sie mit
`401`; die Prüfung liest die Konfiguration frisch von der Platte, damit ein
Widerruf ohne Neustart greift. Der Kern (`tablet/tl.rs`) ist derselbe, den
später auch der Cloud-Weg benutzt — jede Mutation wird genau einmal geprüft,
wie schon bei den Ergebnissen (R5).

Die **Seite selbst** (`GET /tl`) ist bewusst frei zugänglich: Sie enthält
keine Turnierdaten, sondern holt sie erst über die geprüfte Schnittstelle.
Wer sie ohne Zugang öffnet, sieht nur den Hinweis, wie er einen bekommt.
Der Zugang steht beim Koppeln im **Fragment** der Adresse (`…/tl#t=…`) —
ein Fragment wird nie an einen Server gesendet und landet daher in keinem
Protokoll; die Seite legt ihn lokal ab und bereinigt die Adresszeile.

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

## Ergebnis-Übermittlung verlustsicher (Hebel B, ADR 0018)

Der Ergebnis-Weg ist mehrfach abgesichert, damit ein WLAN-/Cloud-Aussetzer kein
Ergebnis verliert und den Schiri nicht blockiert:

- **Der Klick blockiert nicht.** „Ergebnis übermitteln" legt das Ergebnis als
  `pendingResult` in localStorage und sendet im Hintergrund; die UI zeigt sofort
  „wird übermittelt, wird automatisch wiederholt". Wiederholung alle 5 s **bis der
  Server `ok:true` bestätigt** — übersteht Netzausfall, Reconnect und
  Tablet-Reload. Ein **Backstop-Timeout** (~12 s, `AbortController`) bricht einen
  hängenden Einzel-Fetch ab und startet den Retry sofort.
- **Idempotenz (kein Endlos-Retry / Doppel-Write):** Nach einem erfolgreichen
  Write räumt der Server das Feld. Ein Wiederholungs-POST für dasselbe Match mit
  **identischem** Ergebnis (`sets`, Sieger, ScoreStatus) quittiert
  `process_result` deshalb mit `ok` (statt „Kein Match auf diesem Court") — so
  löscht das Tablet `pendingResult` und hört auf zu wiederholen. **R5 bleibt:**
  ein **abweichender** Payload auf ein geräumtes/gewechseltes Feld fällt weiter
  auf Fehler; ein Retry nach Ablauf des kurzen Idempotenz-Fensters (~60 s) auch —
  eine echte spätere Korrektur wird nicht abgewürgt.
- **Host-Retry-Queue auf Platte:** Scheitert der BTP-Write host-seitig, landet das
  Ergebnis in der BTP-Retry-Queue (30-s-Flush). Diese Queue wird **atomar auf
  Platte** persistiert (`btp-retry.json`, turnier-gegated über den
  Turniernamen) und beim Start wieder geladen — so übersteht ein Ergebnis auch
  einen **Host-App-Neustart** (Durabilität: App-Neustart, nicht Stromausfall).
- **Cloud:** Der Relay wartet auf die `ResultAck` nur noch **8 s** (statt 20 s),
  damit `pending`-Slots bei zäher Leitung schneller frei werden; der Client retryt
  ohnehin (idempotent).

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
  je mit Countdown. Während der Pause ist die Zähltafel gesperrt.
  **Seit v0.9.207 endet die Pause nicht mehr automatisch bei 0**
  (ADR [0028](adr/0028-pause-haelt-bis-weiterspielen.md), Spec
  `spielzeiten-prognose` E9): Nach Ablauf zählt die Anzeige rot hoch
  („überzogen +0:37"), bis der Schiedsrichter „Weiterspielen" tippt — die
  Turnierleitung sieht die Überziehung feldgenau in TL-Web. Auch ein
  Reload/eine Geräte-Übernahme behält eine (auch überzogene) Pause bei.
  Der Pausen-Beginn (`startedAt`, Server-Zeit) reist im gespiegelten
  Spielzustand mit; die Behandlungspause erscheint in TL-Web als
  „Behandlung seit …".
- **Spieldauer**: läuft als MM:SS in der Kopfzeile ab Matchstart.
- **Court-Grafik**: zeigt Aufschläger (markiert mit einem **Federball**,
  dessen Korken in Flugrichtung zum diagonal gegenüberliegenden
  Aufschlagfeld zeigt) und Annehmer auf
  dem Spielfeld – im Einzel ein Name je Hälfte, im Doppel zwei.
- **Verein (optional)**: Ist die turnierweite Vereins-Anzeige im Setup
  („Vereine anzeigen") eingeschaltet, steht unter dem Spielernamen klein der
  **Vereinsname** und/oder das **Vereinslogo**. Die beiden Schalter reisen
  **in-band** mit der Paarung (`MatchBrief.show_club_names/logos`), das Tablet
  übernimmt eine Änderung also mit der nächsten Zuweisung ohne Neuladen.
  Standard aus. Logos laden im LAN vom Turnier-PC (`/info/club-logo`,
  offline-fähig, unscharfes Namensmatching); im Cloud-Modus direkt über den
  öffentlichen badhub-Resolver (`/api/v1/club-logo`, exakter Name). Fehlt
  eines, bleibt es beim Namen.
- **Punktverlauf** ([punktverlauf.md](punktverlauf.md)): Das Tablet
  protokolliert jeden Ballwechsel (`rallyLog`, überlebt Reload und
  Geräte-Übernahme) und meldet ihn dem Turnier-PC (`rally`-Frame; nach
  Undo/Reconnect/Übernahme ein kompletter `rally_sync`). Der „📈
  Verlauf"-Knopf am Score-Board zeigt je Satz ein Liniendiagramm — aus
  den lokalen Daten, funktioniert also auch offline.
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
  `target ≥ 99` genügt ein Satz, sobald er **nicht unentschieden** ist),
  kein Satz darf **über dem Deckel** enden, es muss ein **eindeutiger
  Match-Sieger** herauskommen und es dürfen **keine überzähligen Sätze**
  dabei sein (der Sieg muss erst mit dem letzten Satz feststehen) — sonst
  erscheint eine Meldung im Dialog. Die Satzregel ist dieselbe wie
  serverseitig (`server::sets_fit_format`, siehe [walkover.md](walkover.md)).
- **Und serverseitig noch einmal** (R5): `process_result` prüft getippte
  Endstände seit 09.08.2026 gegen dieselbe Zählweise. Vorher hing die
  Prüfung nur am Weg der Turnierleitung — der Tablet-Weg verließ sich
  darauf, dass die Seite nichts Ungültiges zählen lässt. Für *getippte*
  Ergebnisse gilt das nicht: In einem Turnier bis 15 mit Deckel 21 ging ein
  **27:25** durch, direkt nach BTP und in den Liveticker. `setWinnerSide`
  fragt nämlich nur, ob jemand den Deckel *erreicht* hat — beim Live-Zählen
  genügt das, weil dort beim Deckel Schluss ist. Bei **Aufgabe, Kampflos
  und Disqualifikation** entfällt die Prüfung: Dort bricht das Spiel mitten
  im Satz ab, und genau dieser Stand gehört nach BTP.
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

### Reconnect-Wahrheit: der Slot-Halter gewinnt (seit v0.9.197)

Bei störanfälligem WLAN (hunderte Fremdgeräte in der Halle) muss das
zählende Tablet die **Wahrheit des laufenden Spiels** halten. Seit v0.9.197
entscheidet das **nicht mehr** ein geräte-lokaler `rev`-Zähler, sondern der
**Slot-Halter** — wer den Feld-Slot laut Server legitim hält (R4: ein aktives
Tablet je Court). Der Server/Relay **berechnet** die Autorität und schickt sie
beim Reconnect explizit im `state_restore` mit (`authoritative`,
`ownership_active`; ADR 0017):

- Kehrt **dasselbe** Gerät zurück und niemand hat übernommen → es **setzt seinen
  lokalen Stand durch** (Server-Cache + TV + Liveticker ziehen nach).
- Hat ein **anderes** Gerät den Slot übernommen **und weitergezählt** → das
  zurückkehrende Tablet **tritt zurück** und überschreibt den Übernehmer nicht —
  auch wenn sein lokaler `rev` höher ist (das war der Bug der reinen
  `rev`-Lösung: die geerbte gemeinsame Basis machte `rev` geräteübergreifend
  unvergleichbar).
- Bei echter **Divergenz** (beide haben nach einem Split gezählt) gewinnt der
  aktuelle Slot-Halter **deterministisch** — der andere Stand geht bewusst
  **still** verloren (Determinismus statt manueller Auflösung).

**Finalisiert-Schutz:** Wird ein Match in BTP **per Hand fertig eingegeben**
(finalisiert, `MatchStatus::Finished`), reist ein `finalized`-Flag im
Match-Frame zum Tablet. Das Tablet **tritt dann zurück**: es pusht keinen Score
mehr und sendet kein Ergebnis, überbügelt das Hand-Ergebnis also nicht. Der
Server verwirft zusätzlich einen Score für ein bereits finalisiertes Match
(ergänzt `process_result`, R5).

**Rollback im Turnier:** Die Config `reconnect_legacy_rev` (Default aus =
Ownership aktiv) schaltet zur Laufzeit auf das alte `rev`-Verhalten zurück
(unten). Der Server signalisiert das dem Tablet über `ownership_active=false`;
ältere App-Versionen ohne die Felder fallen per `serde(default)` ebenfalls auf
`rev` zurück (Auto-Update-sicher).

#### Altes `rev`-Verhalten (Legacy-Fallback)

Jede lokale Änderung zählt `rev` im persistierten Snapshot hoch. Schickt
der Server beim Reconnect seinen (während des Aussetzers veralteten)
Spielstand (`state_restore`), gilt **„neuer gewinnt"**: Hat das Tablet
zum selben Match einen gleich neuen oder neueren Stand, behält es ihn
und spiegelt ihn sofort zurück — vorher überbügelte der alte Server-Stand
die weitergezählten Punkte (Turnier-Befund 18.07.2026). Ein frisches Gerät
(Reload ohne Stand, Ersatz-Tablet, echte Übernahme) übernimmt den
Server-Stand unverändert. Dieser Pfad greift, wenn `ownership_active=false`
(Legacy-Schalter an oder alte App/altes Relay).

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
