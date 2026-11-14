# Cloud-Relay – Tablets durch jede Firewall

Der digitale Tablet-Spielzettel ([tablet.md](tablet.md)) betreibt im
LAN-Modus einen Server auf dem Turnier-PC (Port 8088), an den sich die
Tablets **eingehend** hängen. Auf IT-verwalteten Rechnern blockiert die
Windows-Firewall diesen eingehenden Port – der Turnierleiter hat keine
Admin-Rechte, das zu ändern. Manche Hallen-WLANs isolieren zusätzlich die
Geräte voneinander. Folge: die Tablets erreichen bts-light nicht.

Der **Cloud-Modus** löst das: Tablet ↔ bts-light läuft nicht mehr direkt,
sondern über einen Relay-Dienst auf badhub.de. bts-light **und** die
Tablets verbinden sich nur noch *nach außen* – eine ausgehende Verbindung
lässt jede Firmen-IT durch (es ist nichts anderes als Surfen). Kein
eingehender Port, keine Admin-Rechte, kein WLAN-Gefummel.

```
Tablet (Browser) ──außen──▶  badhub.de/bts-relay  ◀──außen── bts-light ──▶ BTP (lokal)
```

Der BTP-Schreibweg bleibt lokal auf dem PC: Ein vom Tablet übermitteltes
Ergebnis reicht der Relay an bts-light durch, das es per `SENDUPDATE` nach
BTP schreibt – exakt wie im LAN-Modus.

> **Testsystem.** Steht die Installation auf `test.badhub.de`
> ([testsystem-umschalter](features/testsystem-umschalter.md)), lauten alle
> Relay-Adressen dieser Seite entsprechend `test.badhub.de/bts-relay/…`. Das
> setzt zweierlei voraus: dass dort ein `bts-relay` hinter nginx läuft, **und**
> dass er mit `PUBLIC_BASE=https://test.badhub.de/bts-relay` gestartet wird.
> Ohne die Variable baut der Relay seine QR-Codes weiter mit dem
> Produktiv-Default (`relay/src/main.rs`, `qr_svg`) – ein Tablet scannt dann
> den Code am Testsystem und landet in einem Produktiv-Namespace, auf dem kein
> Host sitzt: Es wartet endlos. Fehlt der Test-Relay ganz, gehört der Testlauf
> auf LAN.

## Umschalten

Die Verbindungsart steht im Setup-Wizard unter **„Tablet-Verbindung"**:

- **LAN – lokales Netz** – schnell und offline, braucht aber den
  freigegebenen Port 8088.
- **Über badhub.de – Cloud** – funktioniert auch hinter gesperrten
  Firewalls, braucht Internet.

Beide Kacheln lassen sich **gleichzeitig** aktivieren – für Zwei-Hallen-
Turniere: Haupthalle per LAN, zweite Halle per Cloud. bts-light startet
dann LAN-Server und Relay-Verbindung zusammen; der Spielzettel zeigt je
Feld beide QR-Codes.

Der Wechsel greift beim nächsten Stoppen/Starten des Livetickers (kein
Live-Umschalten mitten im Betrieb). Beide Wege bleiben dauerhaft nutzbar.

**Traffic** ist minimal: pro Punkt ein WebSocket-Frame von wenigen hundert
Byte – auch bei 20–30 Feldern vernachlässigbar.

## Architektur

`bts-light` ist ein Cargo-Workspace aus drei Crates:

| Crate | Zweck |
|---|---|
| `src-tauri` | die Tauri-Desktop-App (bts-light selbst) |
| `relay` | der Relay-Dienst – Binary `bts-relay` |
| `relay-proto` | die geteilten JSON-Wire-Typen beider Seiten |

Der Relay ist ein reiner WebSocket-Broker ohne Persistenz. Jede
bts-light-Installation hat über ihre `install_id` (zufällige UUID aus der
App-Konfiguration) einen eigenen **Namespace** – Turniere kollidieren
nicht. Pro Namespace gibt es genau einen „Host" (bts-light) und beliebig
viele Tablets, je an einen Court gebunden.

Die Tablet-URL im Cloud-Modus:
`https://badhub.de/bts-relay/<install_id>/court/<court>`

### Endpunkte des Relays

Nach dem nginx-Präfix-Strip (`/bts-relay/` → `/`) sieht der Relay:

| Route | Zweck |
|---|---|
| `GET /{ns}/court/{label}` | Tablet-Spielzettel-UI (dieselbe `tablet.html` wie die App) |
| `GET /{ns}/court/{id}/tafel` | Zähltafel (dieselbe `tafel.html` wie die App); Zuweisung `court_tafel` liefert im Geräte-State Feld-Stand + `redirectTo` |
| `GET /{ns}/anzeige` | Anzeige-Hülle fürs Tablet (dieselbe `anzeige.html`), PIN leer → `0000` |
| `GET /{ns}/courts` | Feldliste fürs Feldwechsel-Menü (vom Host gepusht); seit v0.9.275 mit `occupied` (Belegt-Warnung der Anzeige-Hülle) |
| `GET /{ns}/qr/{label}` | QR-Code (SVG) auf die öffentliche Court-URL |
| `GET /{ns}/ws` | Tablet-WebSocket |
| `GET /{ns}/host-ws` | bts-light-Host-WebSocket (ausgehend) |
| `POST /{ns}/result` | Endergebnis vom Tablet → an den Host weitergereicht |
| `POST /{ns}/pairing-code` | Telefon-Kopplungscode ausstellen (ADR 0004, nur bei verbundenem Host) |
| `GET /pair/{code}` | Telefon-Code → Namespace auflösen (1 h TTL, Fehlversuchs-Limit) |
| `GET /tl`, `GET /tl/api/state`, `POST /tl/api/command` | Turnierleitungs-Oberfläche — **ohne Namespace in der Adresse**, siehe unten |
| `GET /tl-ws` | TL-Push-Anstoß (Spec `tl-web-push`, ADR 0034): sendet nur `{"rev":n}`, Zugang im **ersten Frame** |
| `GET /{ns}/ads/{index}` | Hochgeladenes Werbebild (per **Position** in der Liste) — der Anzeige-Stil reist positionell mit, siehe unten |
| `GET /{ns}/info/logo` | Hochgeladenes Turnierlogo |
| `GET /{ns}/stream?courts=` | Stream-Einblendung für OBS (seit v0.9.328, ADR 0069); holt je Feld `/{ns}/court/{id}/state` |
| `GET /{ns}/info/club-logo?name=` | Vereinslogo der Anzeigen: relativer 302 auf den badhub-Resolver `/api/v1/club-logo?name=` (seit v0.9.327, ADR 0070) |
| `GET /{ns}/info/ad/state` | Werbe-/Leisten-Zustand (`ads`, `barAds`, `streamAds`, `streamVer`, `hasLogo`, `intervalS`, `adStyles`); mit `?zweck=stream` 503 statt „leer", wenn der Relay nichts weiß (ADR 0092) |
| `GET /health` | Status-Schnappschuss |

Die Anzeige-Hülle bettet die Anzeige-Seiten in ein iframe derselben Herkunft
ein. Der nginx vor `/bts-relay/` darf deshalb kein `X-Frame-Options: DENY`
bzw. `Content-Security-Policy: frame-ancestors 'none'` setzen — sonst bleibt
der Rahmen auf den Tablets still leer; `SAMEORIGIN` ist unkritisch. Prüfen
mit `curl -I https://badhub.de/bts-relay/<ns>/court/1/tafel`.

**Deploy-Reihenfolge:** Ein Relay, das `court_tafel` nicht kennt, lehnt den
**gesamten** Zuweisungs-Upload des Hosts mit 422 ab. Dabei frieren **alle**
Zuweisungen und Fernbefehle des Turniers ein — nicht nur die Tafel —, solange
irgendein Gerät auf `court_tafel` steht: Der Host lädt `assignments` und
`targets` in einem Body hoch, das alte Relay verwirft ihn komplett und behält
den zuletzt akzeptierten Stand für alle Geräte. Der Relay wird beim Merge
deployt, der App-Tag folgt danach — ein Relay-Rollback nach einem
App-Release bricht die Monitor-Steuerung.

Die beiden Bild-Routen antworten mit einer Kennung (`ETag`) und dürfen
zwischengespeichert werden — Werbebilder **eine** Minute, das Turnierlogo
fünf. Ein unverändertes Bild kostet dann rund 200 Byte statt der vollen
Nutzlast; vorher (`no-store`) zog jede Anzeige bei jedem Motivwechsel das
ganze Bild erneut über die Internetleitung. Zwei Feinheiten, die hier
anders liegen als im LAN:

- Die Kennung wird beim **Hochladen** aus dem Bildinhalt berechnet, nicht
  aus dem Upload-Vorgang. Der Host lädt sein Monitor-Bündel bei jedem
  Verbindungsaufbau neu hoch — an den Upload gebunden entwertete jeder
  Verbindungsabbruch sämtliche Bild-Zwischenspeicher der Geräte.
- Werbebilder werden über ihre **Position** adressiert. Beim Löschen eines
  Bildes rücken alle folgenden auf, die Adresse zeigt also plötzlich auf ein
  anderes Motiv — daher die kurze Frist. Das Turnierlogo hat eine feste
  Adresse je Namespace.

Kennt der Abrufer die Kennung schon, entfällt zusätzlich das Kopieren der
Bilddaten unter dem globalen Namespace-Mutex. `If-None-Match` wird nach
RFC 9110 ausgewertet (Liste, `*`, schwacher Vergleich) — nginx davor darf
eine Kennung abschwächen, ein reiner Gleichheitstest wäre dann still
wirkungslos. Einzelheiten: [court-monitor.md](court-monitor.md).

### Datenfluss

1. bts-light verbindet sich im Cloud-Modus ausgehend zu
   `wss://badhub.de/bts-relay/<install_id>/host-ws`.
2. Ein Tablet öffnet `…/court/<court>` und verbindet seine WebSocket. Der
   Relay meldet dem Host `tablet_connected`.
3. Der Host pusht alle 2 s die Court→Match-Zuweisung; der Relay leitet sie
   an das jeweilige Tablet.
4. Jeder Punkt am Tablet → `score_update` → Relay → Host → Liveticker.
   **Rückrichtung (v0.9.200):** Der Host spiegelt jeden Feld-Stand als
   `HostFrame::ScoreUpdate` (Satzliste + opaker `court_state`) an den
   Relay — nudge-getrieben (A1-Abo auf dem eigenen Monitor-Kanal) plus
   ein 2-s-Sweep **nach** dem Zuweisungs-Push (fängt Reconnects,
   Court-Wechsel und BTP-Handeingaben ein), dedupliziert per
   Fingerabdruck. Nur so sehen Cloud-Monitor/-Übersicht auch die Stände
   von **LAN**-Tablets (`LanAndCloud`-Mischbetrieb); der Relay übernimmt
   sie mit demselben Stale-Schutz wie beim Tablet-Weg (plus: leere Sätze
   überschreiben keinen Live-Stand, ein `state` mit fremder eingebetteter
   Match-ID wird verworfen) und weckt die Monitor-Abonnenten. Details:
   [court-monitor.md](court-monitor.md) („Score-Spiegel des Hosts").
   Zusätzlich seit dem Punktverlauf-Graph
   ([Spec](features/punktverlauf-graph.md), ADR 0014): je Ballwechsel ein
   `rally`-Frame und nach Undo/Reconnect/Übernahme ein `rally_sync`
   (Komplett-Resync, ersetzt den Host-Stand des Matches). Der Relay
   reicht beide 1:1 durch und prüft nur die Deckel
   (`MAX_RALLIES_PER_SET`, `MAX_TIMELINE_SETS`, `MAX_TIMELINE_LEN`);
   interpretiert wird allein beim Host. Der Abruf für die TL-Oberfläche
   läuft als Request/Response `timeline_request`/`timeline_data` über die
   host-ws (Muster TL-Kommando) — der Relay hält keine Verläufe vor.

   Seit dem Schiedsrichterzettel ([Spec](features/schiedsrichterzettel-druck.md),
   ADR 0037/0038/0039) kommt ein **zweiter, eigener Strom** daneben:
   `match_event` je erfasstem Ereignis (Karte, Verletzung, Unterbrechung,
   Überstimmung, Aufschlagfolge; seit v0.9.319 auch `teams_corrected`,
   ADR 0067) und `match_event_sync` als Abgleich. `EventKind` hat keinen
   Catch-All: Ein Relay oder Host ohne `teams_corrected` verwirft den ganzen
   Frame. Deshalb meldet der Host im `MatchBrief` `teamsCorrection: true`, und
   nur dann bietet das Tablet „Mannschaften tauschen" an. Ausrollen wie
   immer Relay vor App (main-Merge deployt den Relay sofort).
   Getrennt vom Punktverlauf, weil Karten personenbezogene
   Sanktionsdaten sind und `MatchTimeline` personenbezugsfrei bleiben
   muss (ADR 0015) — mit eigenen Deckeln (`MAX_EVENTS_PER_MATCH`,
   `MAX_SHEET_LEN`, `MAX_EVENT_ID_LEN`), damit ein Ereignis-Schwall den
   Punktverlauf strukturell nicht verdrängen kann. **Wichtiger
   Unterschied zu `rally_sync`:** der Abgleich **vereinigt**, er ersetzt
   nicht (ADR 0038) — ein übernehmendes Ersatz-Tablet kennt die Karten
   seines Vorgängers nicht und würde sie sonst löschen. Der Druckabruf
   läuft als `scoresheet_request`/`scoresheet_data` über die host-ws; der
   Relay hält auch **keine Zettel** vor.

   **Vor-Match-Fenster** (seit v0.9.375, Spec
   [tablet-stand-robust](features/tablet-stand-robust.md), Regel D): Die
   beiden **kompletten** Abgleiche `rally_sync` und `match_event_sync`
   reicht der Relay nicht nur fürs Spiel auf dem Feld durch, sondern noch
   **120 s** lang auch für das Spiel, das zuletzt von diesem Feld gegangen
   ist (`MatchCleared` oder `MatchAssigned` mit anderer Match-ID;
   `Namespace.vor_match`, `sync_darf_durch`). Grund: Der Abgleich reist
   über den WebSocket, das Ergebnis über HTTP — nach einem Abriss kommt
   der Abgleich oft erst an, wenn der Host das Feld schon geräumt hat.
   Gemerkt wird nur die Match-ID, kein Inhalt; ob der Abgleich gilt,
   entscheidet weiter allein der Host (eigenes 120-s-Fenster, nur
   ergänzend). Einzelne `rally`- und `match_event`-Frames bekommen das
   Fenster **nicht**. Ein alter Relay verwirft den Nachzügler wie bisher —
   deshalb Relay vor App (der main-Merge deployt ihn sofort).

   Der Relay prüft dabei **eigene** Deckel, nicht die des Punktverlaufs:
   `match_events_valid` (Zahl, `MAX_EVENTS_PER_MATCH`) und `MAX_SHEET_LEN`
   (Größe). Halter- und Stale-Filter sind dieselben wie beim Rally — bei
   Sanktionsdaten wiegt das schwerer als beim Punktverlauf: eine Karte am
   falschen Spiel wäre eine Falschbeschuldigung im Archiv.

   > **Stand:** Wire (E1), Store (E2), Ingest samt Durchleitung (E3),
   > Projektion (E4) und Leseweg (E5) stehen. Die Erfassung am Tablet
   > folgt in E6 — bis dahin erzeugt niemand solche Frames.
   >
   > **Ausrollreihenfolge, ab hier verbindlich:** Ein noch nicht
   > aktualisierter Relay verwirft die neuen Tablet-Frames **still**.
   > Also **erst den Relay ausrollen, dann den App-Tag setzen** — sonst
   > erfassen Tablets Ereignisse, die unterwegs verschwinden.
5. „Ergebnis übermitteln" → `POST …/result` → Relay reicht es per
   WebSocket-Frame an den Host → bts-light schreibt per `SENDUPDATE` nach
   BTP und antwortet mit `ResultAck`. Der Relay wartet auf die `ResultAck` nur
   **8 s** (`RESULT_TIMEOUT`, seit Hebel B / ADR 0018; vorher 20 s) — danach
   `ok:false`, der Client puffert und retryt ohnehin (idempotent). Das kürzere
   Warten gibt den `pending`-Slot (`MAX_PENDING_PER_NS`) schneller frei. Details:
   [tablet.md](tablet.md) („Ergebnis-Übermittlung verlustsicher").

   Die `ResultAck` trägt seit v0.9.254 zusätzlich **`permanent`**: Sagt der
   Host, dass genau dieses Ergebnis dauerhaft abgelehnt ist, reicht der Relay
   das unverändert an das Tablet durch (er urteilt nicht selbst — R5 liegt beim
   Host). Ohne dieses Feld wirkte die Unterscheidung ausgerechnet im
   Cloud-Betrieb nicht, weil der Relay seine Antwort aus `ok`/`error` neu baut.
   Der Normalfall `false` bleibt von der Leitung (`skip_serializing_if`), und
   ein Relay-Timeout bleibt weiterhin **wiederholbar** — er sagt nichts über
   das Ergebnis aus.

   Seit v0.9.376 trägt sie außerdem **`superseded`** („überholt", Spec
   [tablet-stand-robust](features/tablet-stand-robust.md), Regel C): Das
   Spiel liegt nicht mehr auf dem Feld und ist in BTP beendet. Wie
   `permanent` reicht der Relay das Feld nur durch; es ist nie `ok` und nie
   `permanent`, und der Normalfall `false` bleibt von der Leitung. Ein alter
   Relay lässt das Feld fallen — das Tablet liest dann eine wiederholbare
   Absage und parkt den Auftrag (sicherer Rückfall). Deshalb Relay vor App
   (der main-Merge deployt ihn sofort). Bedeutung am Tablet:
   [tablet.md](tablet.md) („Überholt und geparkt").

**Aufgerufene Spiele in die ferne Halle (Cluster C Stufe 2, v0.9.154):**
Der Host pusht seine in Vorbereitung gerufenen Spiele als
`HostFrame::Prepared` (nur bei Änderung, Fingerabdruck) an den Relay; der
hält sie je Namespace (`Namespace.prepared`) und gibt sie in
`GET /{ns}/info/announce/state` **hallengefiltert** als
`AnnounceState.prepared` zurück. Der Cloud-Slave zeigt sie unter
„Aufgerufene Spiele" und sagt den **Zweit-/Drittaufruf** einer fehlenden
Partei **lokal** in seiner Halle an (kein Rückkanal zum Master). Details:
[announcements.md](announcements.md).

**Hallen-Farben (Spec [features/hallen-farben.md](features/hallen-farben.md),
ADR 0033):** Drei bestehende Frames tragen ein **optionales** Farb-Feld als
Hex-String (`#rrggbb`, `#[serde(default)]`/`skip_serializing_if`):
`CourtBrief.hall_color` (im `HostFrame::Courts`-Push),
`PreparedMatch.hallColor` und `MonitorState.hallColor`. Der Relay reicht
die Werte unverändert an `overview_health` (JSON-Schlüssel `hall_color`,
identisch zur LAN-`/health`), `preparation_state` und den Monitor-Zustand
durch — **kein neuer Frame, kein Paletten-Spiegel**. Alte Hosts liefern
das Feld nicht, alte Seiten ignorieren es: Jede Versions-Mischung
degradiert zu „farblos", nie zu „kaputt". TL-Web bekommt die Farben
separat über das opake `TlState`-JSON (`TlHall.color`).
**Deploy-Reihenfolge:** Relay vor App-Release (läuft automatisch beim
main-Merge); die Seiten prüfen die strikte `#rrggbb`-Form, bevor ein Wert
in ein Style-Attribut gelangt.

### Turnierleitungs-Geräte (TL-Web) — Wire-Ebene

> **Stand:** Der Weg steht in beiden Richtungen — im **LAN** (siehe
> [tablet.md](tablet.md)) und über den **Relay**. Der Turnier-PC spiegelt
> seine Zugänge (`TlAuth`) und seinen Anzeige-Zustand (`TlState`) und
> beantwortet `TlCommand` mit **derselben** Ausführung wie im Hallennetz.
> Gekoppelt und widerrufen wird unter **Turnierleitung** in der
> Desktop-App: Name eintragen, QR scannen, fertig. Der Zugang ist genau
> einmal sichtbar — im QR-Code beim Koppeln. Ohne Opt-in
> (`tl_web.enabled`, Default aus) kennt der Relay kein einziges Token:
> **Jede** Anfrage endet abgewiesen, bevor sie irgendetwas berührt.
> Fachliche Grundlage:
> [features/turnierleitung-web.md](features/turnierleitung-web.md),
> [ADR 0011](adr/0011-tl-web-schreibender-cloud-pfad.md) und
> [ADR 0012](adr/0012-tl-web-geraete-identitaet.md).

Turnierleitungs-Geräte sind eine **dritte Client-Klasse** neben Tablets und
Monitoren: mehrere je Namespace (höchstens 8), **nicht** feldgebunden, und
sie schreiben ausschließlich **über den Host**. Sie landen nie in der
Tablet-Liste eines Namespace und übernehmen nie eine Court-Session — R4
(„ein aktives Tablet je Court") bleibt unberührt.

**Anstoß statt Dauerfragen** (Spec
[features/tl-web-push.md](features/tl-web-push.md), ADR 0034, seit
v0.9.221): Neben den HTTP-Routen gibt es `GET /tl-ws`. Der Relay legt
den vom Host gepushten `TlState` wie gehabt ab — und weckt dabei die
Zuhörer dieses Namespace mit `{"rev":n}`, sobald die Revision **neu**
ist. Der Kanal trägt **nie** Turnierdaten; die Seite holt sie über ihr
bestehendes `GET /tl/api/state` (eine Wahrheit für Auth, ETag,
Kürzungsleiter und `X-Tl-Active-Profile`). Eigenheiten:

- **Zugang im ersten Frame** (`{"token":…}`, 10-s-Frist), nicht im Pfad
  und nicht im Query: WebSockets tragen keine Kopfzeilen, und Adressen
  landen in Zugriffsprotokollen. Vor erfolgreicher Prüfung antwortet der
  Relay nichts — auch keinen Grund; unbekannter Zugang oder
  abwesender Host schließen die Verbindung.
- **Deckel 16 Zuhörer je Namespace** (`MAX_TL_SUBS`, doppelter
  Geräte-Cap als Reconnect-Reserve). Darüber bleibt die Verbindung
  ungenutzt, die Seite bedient sich aus ihrem Poll.
- **Der 8-Geräte-Platz hängt weiter am Poll** (`claim_tl_slot`, TTL
  60 s): Ein Kanal allein belegt keinen Platz, der 30-s-Fallback-Poll
  der Seite hält ihn frisch.
- Beim Verbinden schickt der Relay sofort die aktuelle Revision — eine
  Änderung, die während des Verbindens durchrutschte, läge sonst bis
  zum nächsten Fallback-Poll.
- nginx bleibt unangetastet: Der Kanal läuft über die vorhandene
  `Upgrade`-Konfiguration (`ops/nginx-bts-relay.conf`), kein
  `proxy_buffering`-Eingriff nötig — genau der Grund, aus dem SSE hier
  ausscheidet (ADR 0016/0034).

Die geteilten Typen in `relay-proto`:

| Typ | Zweck |
|---|---|
| `TlAction` | Der **geschlossene** Satz erlaubter Aktionen (Feld belegen/räumen/umhängen, Vorbereitungs-Aufruf, erneuter Aufruf — wahlweise je Partei, siehe unten —, Ergebnis, Walkover, Zähltafelbediener, Auto-Vergabe). Was hier nicht steht, ist nicht darstellbar. |
| `CourtExpectation` | Was das Gerät auf dem Feld **vorgefunden** hat (`any` / `free` / `match`). Stimmt es nicht mehr, lehnt der Host ab — so überschreiben zwei Geräte einander nicht stillschweigend. |
| `TlResponse` + `TlErrorCode` | Antwort mit **maschinenlesbarem** Grund, damit die Seite gezielt reagieren kann, plus der Revision, auf die sie sich neu ausrichten soll. Kennt auch „ausgeführt, aber mit Hinweis" (etwa: in dieser Halle ist kein Ansage-Gerät verbunden) — ausdrücklich kein Fehler. |
| `RelayFrame::TlCommand` | Kommando an den Host; `reqId` korreliert die Antwort, `opId` ist der Idempotenzschlüssel gegen doppelte Schreibvorgänge nach einem Netzwackler, `viewRev` die Revision der Ansicht, auf der die Aktion beruhte (Grundlage der Altersprüfung). |
| `HostFrame::TlAck` | Die Quittung — der Absender erfährt das Ergebnis **nach** dem BTP-Schreiben, kein Fire-and-forget. |
| `HostFrame::TlAuth` | Die zugelassenen Geräte als Paare aus **Kennung und Zugang** — ohne Namen (Datensparsamkeit). Der Host stellt sie aus, der Relay spiegelt sie nur; die Liste **ersetzt** die bisherige, und genau das ist der Widerruf. Die Kennung reist mit jedem Kommando zurück, damit das Protokoll des Turnier-PCs benennen kann, wer gehandelt hat, ohne dass ein Zugang in Protokollen auftaucht. |
| `HostFrame::TlState` | Der Anzeige-Zustand als **opakes** JSON plus Revision. Der Relay legt ihn nur ab und liefert ihn unverändert aus — wie beim Court-Zustand bleibt die Turnierlogik vollständig im Host. |

**Die `install_id` verlässt den Master dabei nicht.** Anders als bei
Tablets und Monitoren ist der Namespace kein Bestandteil der
TL-Adressen; der Relay schlägt ihn über das Gerätetoken nach (ADR 0012).
Der Grund ist handfest: Die `install_id` **ist** der Zugang der Zähltablets
(`/{ns}/ws`). Stünde sie in der Adresse, die jeder Helfer den ganzen Tag auf
dem Bildschirm hat, könnte sich damit jeder als Tablet ausgeben.

**Routen im Relay** (seit Schritt 10, alle ohne Namespace):

| Route | Zweck |
|---|---|
| `GET /tl` | Die Seite selbst — **ohne** Zugangsprüfung, wie die Tablet-Seite. Ausgeliefert wird eine leere Hülle, die ihren Zugang erst aus dem Adress-Fragment (`#t=…`) liest; alles Verwertbare kommt über die geschützten API-Routen. |
| `GET /tl/api/state` | Der zuletzt gepushte Anzeige-Zustand, **unverändert** durchgereicht. Mit `ETag` aus **Host-Generation und Revision**: Ein Gerät, das denselben Stand schon hat, bekommt `304` — bei einer Seite, die alle zwei Sekunden fragt, ist das über Mobilfunk der Unterschied zwischen sparsam und lästig. Die Generation muss hinein, weil die Revision beim Neustart des Turnier-PCs wieder klein beginnt; ohne sie bekäme ein Gerät „unverändert" auf einen völlig anderen Turnierstand. Fehlt der Stand, ist die Antwort `503` und **nicht** ein leeres Turnier: Leer sähe aus wie „alle Felder frei". |
| `POST /tl/api/command` | Kommando an den Turnier-PC, Antwort synchron über `reqId`/`TlAck` — dasselbe erprobte Muster wie die Ergebnismeldung vom Tablet, mit 20 s Zeitablauf. |
| `GET /flags/{code}.svg` | Länderflaggen für die TL-Seite. Sie leitet die Flaggen-Basis aus ihrem eigenen Pfad ab (`/bts-relay/tl` → `/bts-relay/flags/`) — und hängt ohne Namespace in der Adresse, deshalb braucht sie diese ns-lose Route neben `/{ns}/flags/…` (Court-Monitor). Statische SVGs ohne Turnierbezug; ein Namespace hätte nichts abzusichern. |
| `GET /tl/api/scoresheet/{ids}` | Schiedsrichterzettel als fertiges HTML, **on-demand** ([schiedsrichterzettel-druck](features/schiedsrichterzettel-druck.md), ADR 0039): Anfrage als `scoresheet_request` über die host-ws zum Turnier-PC, dessen `scoresheet_data`-Antwort zurück an den wartenden Abruf. `{ids}` ist eine Komma-Liste (Stapeldruck einer Runde), gedeckelt auf `MAX_SHEETS_PER_DOC` — der Deckel greift **vor** jeder Arbeit je Kennung. Der Relay hält **keine** Zettel vor: Sie tragen Namen **und** Sanktionsdaten. `found:false` → 404; keine Antwort (auch: älterer Host) → 503. **`?vorab=1`** ([schiedsrichterzettel-autodruck](features/schiedsrichterzettel-autodruck.md)) reist als Flagge `vorab` im `scoresheet_request` mit und liefert den **Vorabzettel** eines noch ausstehenden Spiels — Kopf gefüllt, Raster leer. Der Relay deutet den Wert nicht, er reicht ihn durch; welches Blatt daraus wird, entscheidet der Host. Die Flagge trägt `#[serde(default)]`, ein Host älterer Version antwortet also weiterhin nach dem gewohnten Verhalten (ohne Aufzeichnung: `found:false`). Nur `1`, `true` oder `ja` schalten um; alles andere bedeutet „wie bisher". |
| `GET /tl/api/timeline/{match_id}` | Punktverlauf eines Spiels, **on-demand** ([punktverlauf.md](punktverlauf.md)): Anfrage als `timeline_request` über die host-ws zum Turnier-PC, dessen `timeline_data`-Antwort zurück an den wartenden Abruf — Muster TL-Kommando, der Relay hält keine Verläufe vor. `found:false` → 404 (Papier-Spiel); keine Antwort (auch: älterer Host) → 503 mit Versions-Hinweis. |

Der Relay ist dabei **Briefträger, nicht Schiedsrichter**: Er kennt weder
Spiele noch Felder, prüft den Zugang und reicht durch. Ob eine Aktion
zulässig ist, entscheidet allein der Turnier-PC (R5) — er ist der Einzige mit
dem Turnierstand. Ein Fehler im Relay kann deshalb keine Wertung erfinden.

**Was der Turnier-PC dazu tut** (`relay_client.rs`, 2-s-Takt):

- `TlAuth` — die Zugänge, **nur bei Änderung**, aber immer einmal nach dem
  Verbinden: Der Relay vergisst sie beim Abriss, und ohne diesen ersten Push
  bliebe die Oberfläche nach jedem Reconnect ausgesperrt. Auch die leere
  Liste wird geschickt; sie ist der Widerruf des letzten Geräts und die
  Wirkung des Ausschalters.
- `TlState` — der Anzeige-Zustand, **nur wenn die Revision steigt**. Sonst
  liefe alle zwei Sekunden ein voller Turnierstand durchs Netz, auf
  Mobilfunkgeräten der Turnierleitung.
- `TlCommand` → `tl::execute` → `TlAck`: derselbe Ausführungsweg wie im
  Hallennetz. Die Kennung aus dem Kommando wird gegen die **eigene**
  Geräteliste gehalten, bevor irgendetwas geschieht — sonst hinge die
  Nachvollziehbarkeit („wer hat was ausgelöst") allein am Relay.

**Die Revision zählt der Turnier-PC, für beide Wege dieselbe**
(`tl::build_state_with_rev`). Zwei getrennte Zähler wären schlimmer als
keiner: Ein Gerät im Hallennetz und eines aus dem Internet meinten mit
derselben Zahl verschiedene Stände. Der Fingerabdruck lässt Uhrzeit und
Revision selbst außen vor — sonst zählte sie im Sekundentakt hoch, obwohl
sich nichts geändert hat. Seit der Startzeit-Prognose (Spec
`spielzeiten-prognose`) gilt das auch für `predicted_start_ms` an den
Wartelisten-Einträgen: Der Wert ist zeitabgeleitet und bewegt die Revision
nicht; die Seite klemmt eine dadurch ältere Prognose selbst auf „gleich".
Die neuen `TlState`-Felder (`predicted_*`, `brutto_mins`/`netto_mins`,
`time_stats`) sind **additiv** — der Zustand reist weiterhin als opakes
JSON, der Relay bleibt unverändert; alte `tl.html`-Stände ignorieren sie.
Cloud-Geräte sehen die neue Anzeige erst nach einem **Relay-Deploy**
(`tl.html` ist einkompiliert) — Deploy vor dem Client-Release.

**Spielliste ohne Hinweistexte** (17.08.2026): Drei weitere additive
`TlState`-Felder an den Wartelisten-Einträgen — `team1_ids`/`team2_ids`
(BTP-Lizenznummern, parallel zu den Namen; Link-Ziel der
badhub-Spielerseite, bewusste Datenschutz-Freigabe wie Nation/Verein) und
`blocked.player_keys` (`assign::player_key`-Schlüssel für die punktgenaue
Namens-Färbung; eine neue Seite an einem alten Host fällt auf den
Namensvergleich zurück). Der Zustand bleibt opakes JSON, der Relay
unverändert — aber auch hier: **Relay-Deploy** nötig, damit Cloud-Geräte
die neue Anzeige (Farb-Marken, Eieruhr, Links) bekommen.

**Spielerlinks überall** (18.08.2026, Spec `tl-sicht-feinschliff` Punkt 4):
`team1_ids`/`team2_ids` stehen zusätzlich an den **Feld-Kacheln**
(`TlCourt`) und den **beendeten Spielen** (`TlFinished`) — beide additiv
mit `#[serde(default)]`, der Relay bleibt unverändert, Relay-Deploy nur
für die Anzeige nötig.

**Größenwirkung — die Kürzungskaskade hat eine Stufe dazubekommen.** Bei 26
Feldern und 30 Ergebnissen sind das bis zu 112 zusätzliche String-Arrays.
Der Größen-Wächter maß den Fall bisher gar nicht: Sein Fixture kannte nur
die Warteliste, keine belegten Felder, keine Ergebnisse — und keine
**Vereinsnamen**, die unabhängig von `display.show_club_names` **immer**
mitreisen. Mit realistischem Fixture (26 Felder, 30 Ergebnisse, 400
wartende, Doppelpaarungen mit Verein und Lizenznummer, 40 Schiedsrichter,
20 Zähltafelbediener) liegt der Stand bei **62 467 von 65 536 Bytes** —
rund 5 % Reserve, Warteliste bereits auf unterster Stufe.

Die Kaskade lautet deshalb jetzt: **`queue` (40/20/10/5) →
`checkin_times` → `finished` (10/3)**. Die Ergebnisliste ist reine
Rückschau; die Feldkacheln bleiben in jeder Stufe vollständig, weil sie
das Bedienelement der Seite sind. Ohne die neue Stufe reißt ein
40-Felder-Turnier die Grenze mit 73 942 Bytes — der Relay verwirft dann
das ganze Frame samt Vorgänger, und die Cloud-Turnierleitung sieht **gar
nichts** mehr. Wer den Zustand um eine weitere Liste erweitert (nächster
Kandidat: die vierachsige Spielzeiten-Statistik aus Punkt 1), misst nach
und nimmt sie in die Kaskade auf — die verbleibende Reserve trägt keine
zweite Erweiterung.

**Wunschfeld** (Spec `tl-wunschfeld`, seit v0.9.262): eine neue
`TlAction`-Variante `set_wish_court { matchId, courtId? }` — ohne `courtId`
wird der Wunsch aufgehoben. Der Relay braucht keine Code-Änderung (typisiertes
`TlAction` über `relay-proto`, gemeinsam mit `tl.html` deployt). Additiv im
`TlState`: `TlMatch.wish_court` und das Fähigkeitsmerkmal
`can_set_wish_court`, über das die Seite den Wähler nur zeigt, wenn der Host
die Aktion kennt.

**Zeilenfarbe** (Spec `tl-zeilenfarbe`, seit v0.9.277): eine neue
`TlAction`-Variante `set_highlight { matchId, highlight }` mit `highlight`
0–6 (`MAX_HIGHLIGHT`; BTPs „Hervorheben"). Der Relay trägt sie nur durch
(neues `relay-proto`, Deploy beim Merge). Additiv im `TlState`:
`highlight` an `TlMatch`, `TlOpenMatch` und `TlCourt` (bei 0 weggelassen)
sowie das Fähigkeitsmerkmal `can_set_highlight` — Muster wie beim
Wunschfeld.

**Warnung „Ergebnis fehlt"** (Spec `tl-warnung-fertiges-spiel`, seit
v0.9.259): rein additiv im `TlState` — `TlCourt.decided_since_ms` (Zeitstempel)
und `finished_warning_seconds` (Frist). Keine neue Aktion, keine Relay-Änderung.
Eine neue Seite an einem alten Host bekommt die Felder nicht und warnt
schlicht nicht (`#[serde(default)]`).

**Tablets neu laden** (Spec `tablet-version-abgleich`, seit v0.9.268): der
einzige Weg in dieser Reihe, der den Relay **doch** anfassen muss — er ist
der einzige, bei dem ein Frame vom Host an **alle** Tablets eines Namespace
geht statt an eines.

Drei Frames sind beteiligt: `TlAction::reload_tablets` (Gerät → Host, ohne
Nutzlast), `HostFrame::ReloadTablets` (Host → Relay, ebenfalls ohne Nutzlast)
und `ServerMsg::reload { marke }` (Server → Tablet). Der Relay setzt das
Host-Frame in ein `reload` an **jede** seiner Tablet-Verbindungen um.

**Die Marke setzt jeder Server aus seiner eigenen Seite** — deshalb reist sie
nicht im `HostFrame` mit. Der Relay bettet seinen eigenen Stand von
`tablet.html` ein, und die Cloud-Tablets haben die Seite von ihm geladen, nicht
vom Turnier-PC. Käme dessen Marke an, hielte sich jede Seite unmittelbar nach
dem Neuladen wieder für veraltet.

Vom Host zum Relay geht der Befehl über einen **Zähler**, nicht über einen
Kanal: Der `ReloadWacht` in `relay_client.rs` meldet im Sweep, wenn
`tablet_reload_gen` gestiegen ist.

Er lebt dort **über alle Sitzungen hinweg** (angelegt in `run()`, als `&mut`
durch `serve()` gereicht) — anders als am Tablet-Socket, wo er je Verbindung
neu gesetzt wird. Der Unterschied ist wesentlich: Am Tablet heißt „verbunden"
= „Seite frisch geladen", ein Befehl von davor gilt zu Recht nicht mehr. Hier
heißt es nur „der **Host** hat wieder Leitung", während die Tablets dahinter
gerade *nicht* frisch sind. So löst ein Netzwackler nichts aus, ein Befehl
während eines Abrisses wird aber nachgeholt.

Wie bei der Feldsperre trägt der `TlState` ein additives Merkmal
(`can_reload_tablets`), damit eine neue Seite an einem älteren Host keinen
toten Knopf zeigt.

**Feldsperre** (Spec `tl-web-felder-sperren`, seit v0.9.258): eine neue
`TlAction`-Variante `lock_court { courtId, locked }`. Der Relay braucht dafür
**keine** Code-Änderung — er parst Aktionen typisiert und bekommt die Variante
über `relay-proto` mit; Relay und `tl.html` stammen ohnehin aus demselben
Merge, der Relay ist also nie älter als die Seite.

Die Schere liegt woanders: Der **Host** kommt erst mit einem Release-Tag, und
ein älterer Host verwirft eine unbekannte `TlAction` **still**
(`relay_client.rs`) — das Gerät bekäme nicht einmal eine Fehlermeldung.
Deshalb trägt der `TlState` das additive Feld `can_lock_courts`, und die Seite
zeigt den Menüeintrag nur, wenn es ankommt (Feature-Detection wie bei
`hall_prefill`). Damit ist der Merge vom Release-Tag entkoppelt.

**Hallen-Vorverteilung** (Spec `hallen-vorverteilung`): zwei neue
`TlAction`-Varianten `set_hall_prefill { enabled, window }` und
`clear_auto_halls` — der Relay parst Aktionen **typisiert**, ein alter
Relay lehnt sie ab → auch hier Relay-Deploy vor dem Client-Release. Das
zusätzliche `TlState`-Feld `hall_prefill` und der neue
`hall_source`-Wire-Wert `"auto"` sind additiv; alte Seiten tolerieren
beide (Feature-Detection über das State-Feld — eine neue Seite an einem
alten Host zeigt die Bedienelemente gar nicht).

**Spielzeiten-Auswertung mehrachsig** (Spec `tl-sicht-feinschliff` Punkt 1,
v0.9.231): `TlState.time_stats` trägt zusätzlich `by_class`,
`by_discipline` und `by_hall`, jede Zeile zusätzlich `hall`; das Profil trägt
`timeStatsAxis`. Alles additiv mit `#[serde(default)]` — alte Seiten
ignorieren die neuen Felder, ein altes Profil liest sich als `group` (die
bisherige Ansicht). Der Relay bleibt unverändert; **Relay-Deploy** nur nötig,
damit Cloud-Geräte die neue Anzeige bekommen.

Alle vier Achsen reisen **gemeinsam** mit, obwohl je Gerät nur eine gezeigt
wird: Der Zustand entsteht seit ADR 0034 einmal zentral für alle Geräte,
während die Achsen-Wahl im Profil je Gerät liegt. Der Host könnte also gar
nicht nur die gewählte liefern — das Umschalten ist dafür ein reiner
Client-Vorgang ohne Rückfrage.

**Größenwirkung — die Kürzungskaskade hat eine weitere Stufe bekommen.** Die
Auswertung wiegt bei einem großen Turnier mehr als die Warteliste: gemessen
**14 760 Bytes** bei 110/22/5/2 Zeilen (22 Klassen × 5 Disziplinen, zwei
Hallen). Die Kaskade lautet deshalb jetzt **`queue` (40/20/10/5) →
`checkin_times` → `time_stats`**. Ohne die letzte Stufe ginge ein solcher
Zustand mit gut **70 000 von erlaubten 65 536 Bytes** hinaus — der Relay
verwürfe ihn samt Vorgänger, und die Cloud-Turnierleitung sähe gar nichts
mehr, auch keine Felder. Mit ihr sind es 55 286 Bytes; das Panel verschwindet
dann ehrlich, statt den ganzen Stand zu kippen. Rückschau ist verzichtbar,
Bedienung nicht.

Das Feld `hall` reist dabei nur auf der Hallen-Achse mit
(`skip_serializing_if`): Auf den anderen drei wäre es ein leerer String je
Zeile — beim Messfixture allein 1 370 Bytes. Das geht nur, weil das Feld neu
ist; `class_label`/`discipline` lassen sich nicht weglassen, alte Seiten
lesen sie unbedingt.
**Spielbeginn-Ansage** (Spec `tl-sicht-feinschliff` Punkt 3, v0.9.230): eine
neue `TlAction`-Variante `announce_start_play { courtId, matchId }`. Der
Relay parst Aktionen **typisiert** — ein alter Relay antwortet **422**, die
Seite zeigt dann „Der Turnier-PC hat die Anfrage abgewiesen (422)". Also
auch hier: **Relay-Deploy vor dem Client-Release**. Der Zustand wächst
nicht mit; der Knopf steht im ⋯-Menü und braucht kein neues `TlState`-Feld.

Dazu ein neuer **Ansage-Auftragstyp** `start_play` (`AnnounceJobKind`,
`tablet/state.rs`). Der reist **nicht** über den Relay, sondern nur vom
Master zu den Ansage-Geräten seiner Halle — im LAN über
`/info/announce/jobs`. Wichtig für den Mischbetrieb: Ein Ansage-Slave mit
älterem Stand überspringt seit v0.9.230 unbekannte Auftragstypen einzeln,
statt an einem die ganze Charge zu verlieren (siehe
[announcements.md](announcements.md)). In einer per Relay angebundenen
**fernen** Halle erklingt die Ansage weiterhin gar nicht — dort holt der
Slave bewusst keine Aufträge ab.
**Komplett-Ansage** (v0.9.370): eine neue `TlAction`-Variante
`announce_court_full { courtId, matchId }`. Der Relay parst Aktionen
**typisiert** — ein alter Relay antwortet **422**, also **Relay-Deploy vor
dem Client-Release** (geschieht automatisch beim main-Merge). Der Host legt
einen gewöhnlichen `court_call`-Auftrag mit Stufe 4 ab (= vollständige
Ansage ohne Stufenwort, beide Parteien), ohne den Aufruf-Zähler anzufassen —
kein neuer Auftragstyp, alle Ansage-Geräte können ihn schon. Der Zustand
trägt das Fähigkeitsmerkmal `can_announce_court_full`; fehlt es (älterer
Host, der die Aktion still verwürfe), zeigt die Seite den Eintrag nicht.
**Nachruf an die Zähltafelbedienung** (Spec `tl-sicht-feinschliff` Punkt 2,
v0.9.232): eine neue `TlAction`-Variante `announce_scorekeeper { courtId }`.
Der Relay parst Aktionen **typisiert** — ein alter Relay antwortet **422**.
Also auch hier: **Relay-Deploy vor dem Client-Release**. Der Zustand wächst
nicht mit; der Zählerstand bleibt bewusst am Host (er bestimmt allein die
Ansage-Stufe).

Dazu ein neuer **Ansage-Auftragstyp** `scorekeeper_call`, der wie alle
Ansage-Aufträge nur vom Master zu den Ansage-Geräten seiner Halle reist,
nicht über den Relay.

**`viewRev` wird bewusst nicht gegen eine Schwelle geprüft.** Eine Grenze in
Revisionen wäre willkürlich: Sie steigt bei jeder Änderung, in einem vollen
Turnier im Sekundentakt, in einer ruhigen Phase minutenlang gar nicht —
dieselbe Zahl bedeutete mal Sekunden, mal eine Viertelstunde. Was ein
veralteter Blick anrichten kann, fangen die fachlichen Prüfungen genauer ab:
`expect` beim Feld, der beanspruchte Walkover-Vorschlag, die
Ergebnisprüfung. Die Zahl dient der Nachvollziehbarkeit.

Zwei Hürden vor dem Schreibweg: Der Zugang muss (1) im **Wegweiser** ein
Turnier finden und (2) in genau diesem Turnier eingetragen sein. Ein Zugang
aus dem Turnier nebenan scheitert an der zweiten. Ein Zugang, den ein
anderes Turnier bereits belegt, wird **nicht** übernommen (der bestehende
Eintrag gewinnt, mit Warnung im Log) — sonst könnte ein zweiter Namespace
ein fremdes Gerät zu sich umleiten.

**`401` heißt „entzogen", `503` heißt „Turnier-PC weg" — und die
Unterscheidung ist keine Kosmetik.** Ohne verbundenen Turnier-PC weiß der
Relay über einen Zugang gar nichts (mit dem Host verfallen seine Zugänge),
also antwortet er `503`. Antwortete er dort `401`, würde jeder Netzwackler
des Turnier-PCs wie ein Widerruf aussehen; die Seite verlöre reihenweise
ihre Kopplungen und jedes Gerät müsste mitten im Turnier neu gescannt
werden. Aus demselben Grund wirft die Seite ihren Zugang bei `401` **nicht**
weg, sondern zeigt die Meldung und versucht es weiter. Nebeneffekt: Wer
Zugänge durchprobiert, lernt aus der Antwort nichts über ihre Existenz.

Weitere Grenzen: höchstens **8 gleichzeitige Geräte** je Turnier (ein Platz
wird nach 60 s Stille wieder frei, damit ein geschlossener Tab niemanden
aussperrt), höchstens 64 gespiegelte Zugänge, höchstens 16 offene Anfragen,
Zustand auf 64 KB begrenzt. Reißt eine Grenze, wird das **ganze Frame**
verworfen statt gekappt: Eine halbierte Zugangsliste hieße ein halbierter
Widerruf. Ein verworfener Zustand nimmt auch den vorherigen mit — sonst
bekäme jedes Gerät weiter „unverändert" auf einen eingefrorenen Feldplan und
läse dazu „aktuell".

**Not-Aus:** `BTS_RELAY_TL=off` lässt diese TL-Routen gar nicht erst
entstehen — ohne Rebuild und ohne die übrigen Dienste anzufassen. Der Relay
ist ein globales Binary für alle Installationen; ein Fehler im neuen
Schreibweg muss sich abschalten lassen, während anderswo Turniere laufen.
Genau **ein** Wort schaltet ab (`off`), damit ein Tippfehler in der Umgebung
nicht stillschweigend die halbe Turnierleitung lahmlegt.

Am Turnier-PC stehen die gekoppelten Geräte unter `tl_web` in der
`config.json` (`enabled` plus je Gerät Kennung, Token, Anzeigename,
Kopplungszeit und optionale Halle). Sie überleben damit einen App-Neustart.
Ein **Identitäts-Umzug** (ADR 0006) nimmt sie **nicht** mit: Sonst bliebe
der alte PC über die exportierten Tokens schreibberechtigt, und das Bündel
wäre zugleich ein Satz gültiger Zugänge. Die Geräte koppeln sich am neuen
PC neu — ein Scan je Gerät. Der Schalter selbst wandert mit.

**Kein `#[serde(default)]` auf den TL-Feldern — anders als bei den
Tablet-Typen.** Dort schützt der Default ältere Geräte, die im Feld sind.
Hier gibt es keine ältere Gegenstelle: Die Typen sind neu, und ein
stillschweigend ergänzter Wert würde jeweils die *weitreichendere* oder
*ungeprüfte* Variante auslösen — ein fehlendes `expect` schaltete den
Konfliktschutz ab, ein fehlendes `tokens` sperrte alle Geräte aus, ein
fehlendes `side` riefe beide Parteien statt der einen fehlenden. Solche
Frames werden verworfen, statt geraten zu werden.

Reconnect: bts-light verbindet bei Abriss mit Backoff (1 s → 30 s) neu,
Tablets ebenso. Der 2-s-Ticker re-synct danach den Stand.

**Tablet-Reconnect ≠ Übernahme (seit v0.9.147):** Der Relay merkt sich je
Feld die persistente Geräte-Kennung (`deviceId`) des aktiven Tablets.
Meldet sich **dasselbe** Gerät nach einem Netz-Aussetzer erneut, ersetzt
es seine tote Vorgänger-Session nahtlos (kein „Feld belegt"); fremde
Geräte sehen weiterhin den Übernehmen-Dialog. Den gespiegelten Spielstand
schickt der Relay als `state_restore`.

**Reconnect-Wahrheit „Slot-Halter gewinnt" (seit v0.9.197, ADR 0017):** Die
Autorität, wessen Stand beim Reconnect gilt, berechnet der Relay selbst — er
kennt den Slot-Halter je Feld über `tablet_devices`. Das `state_restore`-Frame
trägt dafür zwei neue Felder (`#[serde(default)]`, abwärtskompatibel):
`authoritative` (das Tablet setzt seinen lokalen Stand durch bzw. adoptiert den
Relay-Stand) und `ownership_active` (Schalter: neues Ownership-Verhalten vs.
altes `rev`). „Legitim weitergezählt" leitet der Relay konservativ aus
`court_scores` ab (nicht-leerer Live-Stand ⇒ ein Übernehmer wird **nie**
überschrieben). Das **Finalisiert**-Flag reist im `MatchBrief` (aus dem
BTP-Status des Hosts). **Legacy-Rollback im Cloud:** der Host reicht
`reconnect_legacy_rev` im `HostFrame::Courts`-Push mit (wie `azureTts`); der
Relay setzt daraus `ownership_active`. Ältere Tablets/Relays ohne die Felder
fallen per `serde(default)` auf das alte `rev`-Verhalten zurück. Details:
[tablet.md](tablet.md).

**Über Kreuz: der Zustand eines Cloud-Tablets beim Host (seit v0.9.378,
ADR 0083).** Bisher lief der gespiegelte Zustand nur in eine Richtung: Der
Host spiegelt LAN-Zustände an den Relay (`HostFrame::ScoreUpdate.state`),
aber den Zustand eines **Cloud**-Tablets kannte allein der Relay. Ein
LAN-Tablet, das so ein Feld übernahm, bekam ihn nicht, und ein
Relay-Neustart löschte die einzige Kopie. Neu:

- `RelayFrame::StateSync { courtId, matchId, state }`: Der Relay reicht
  jeden **angenommenen** Zustand eines Cloud-Tablets (Halter-, Stale- und
  Größenprüfung in `store_court_state` bestanden) an den Host weiter — nur
  bei echter Änderung, nur wenn ein Host verbunden ist. `matchId` ist das
  Spiel, das der Relay auf dem Feld führt. Die Weitergabe ist **gebremst**
  (`host_weitergabe_erlaubt`: je Feld ein Schwung von sechs, danach zwei je
  Sekunde): Ein Zähl-Tablet spiegelt einmal je Punkt und kommt nie an die
  Grenze; ein Gerät im Dauerfeuer träfe sonst mit jedem Frame den
  Turnier-PC. Der Relay selbst nimmt weiter jeden Zustand an.
- Der Host legt ihn mit der Herkunft „Cloud" ab
  (`TabletState::uebernimm_cloud_stand`, `StandQuelle`), nach demselben
  Stale-Filter wie im LAN (Match-ID des Relays **und** die im Zustand). Der
  Text muss ein JSON-Objekt sein. Ein LAN-Tablet, das übernimmt, bekommt ihn
  per `state_restore`; LAN-Monitore sehen Aufschlag und Pause (in der
  gekürzten Fassung, wie bei LAN-Zuständen).

Drei Riegel gegen gegenseitiges Überschreiben:

1. **Kein Echo:** Ein Zustand der Herkunft „Cloud" geht nicht im
   Score-Spiegel zurück (`score_mirror_of` liefert `state: None`) — außer
   **einmal je Feld und Relay-Verbindung** (`score_mirror_frame`,
   `rueckgespiegelt`). So kommt der Zustand nach einem Relay-Neustart
   zurück; die Sätze reisen wie bisher bei jeder Änderung.
2. **Cloud-Tablet spiegelt gerade:** Der Relay überschreibt `court_state` im
   `ScoreUpdate`-Arm nicht, wenn für das Feld ein Tablet angemeldet ist
   **und** es in den letzten 15 s selbst einen Zustand geschickt hat
   (`RIEGEL2_FENSTER`, `tablet_state_seit`). Der Satzstand des Spiegels gilt
   weiter. Die Frist ist Absicht: Ein Cloud-Tablet, das nur noch verbunden
   herumliegt, während ein LAN-Tablet das Feld übernommen hat, fröre sonst
   den Zustand im Relay ein, und Cloud-Monitore zeigten den alten Aufschlag.
3. **LAN-Tablet hält das Feld:** Der Host verwirft den Cloud-Zustand
   (`court_occupied`) — und noch 15 s nach dem letzten LAN-Zustand
   (`LAN_VORRANG`), damit ein kurz weggebrochenes LAN-Tablet bei seiner
   Rückkehr den eigenen Stand vorfindet. Zählen ein LAN- und ein Cloud-Tablet
   gleichzeitig dasselbe Feld, bleibt das ungelöst — es gibt eine Warnung je
   Feld und Spiel im Log des Turnier-PCs.

Die Herkunft zieht bei einem Feldwechsel mit um.

Kosten: Jeder Cloud-Zustand (≤ 40 KB je Punkt, gemessen 24–36 KB) geht
zusätzlich vom Relay zum Host — gerechnet einige KB/s bei zehn Feldern,
nicht gemessen. Ein älterer Host kennt das Frame nicht und überliest es;
ein älterer Relay schickt es nicht. Tests:
`store_court_state_reicht_an_host_weiter`,
`host_spiegel_ueberschreibt_aktives_cloud_tablet_nicht`,
`host_uebernimmt_cloud_stand_mit_stale_filter`,
`score_mirror_ohne_echo_bei_cloud_quelle`,
`cloud_stand_verworfen_bei_aktivem_lan_halter`,
`erster_spiegel_nach_relay_neustart_traegt_stand`,
`state_weitergabe_an_den_host_ist_gebremst`,
`cloud_stand_muss_ein_json_objekt_sein`,
`relay_frame_state_sync_roundtrip`.

**Host-Sätze (seit v0.9.377, ADR 0083):** `HostFrame::MatchAssigned` und
`ServerMsg::MatchAssigned` tragen auf **Frame**-Ebene `hostSets` — die Sätze,
die der Host für das Spiel selbst kennt (abgeschlossene plus laufender,
Team-Koordinaten). Der Relay reicht sie an das Tablet durch und **speichert
sie nicht**: In `court_matches` liegt weiter nur der `MatchBrief`, und zum
Anzeige-Stand (`court_scores`) werden sie nicht. Leer = nichts bekannt,
Legacy-Schalter oder älterer Host; das Feld bleibt dann von der Leitung. Ein
älterer Relay lässt es fallen — das Tablet lädt dann wie bisher bei 0:0.
Deshalb Relay vor App (der main-Merge deployt ihn sofort). Was das Tablet
damit tut: [tablet.md](tablet.md) („der weitere Stand gilt"). Der Relay
selbst rechnet `authoritative` unverändert; das Tablet folgt dem Bit nur
noch, wo es keinen eigenen Stand zum Vergleichen hat.

### Erneute Aufrufe — je Partei

Beide Aufruf-Aktionen können auf **eine Partei** eingegrenzt werden; die
Partei reist als `relay_proto::PrepCallSide` (`"both"` / `"team1"` /
`"team2"`).

| `TlAction` | Partei-Feld | Verhalten |
|---|---|---|
| `announce_prep_call` | `side` — **Pflicht** | Nachruf am Meeting Point. Die Stufe wird **je Partei** gezählt (`prep_call_stages`); die eine kann längst da sein, während die andere fehlt. |
| `repeat_prep_call` | `matchId` | Erste Vorbereitungs-Ansage unverändert wiederholen (Auftrag `meetingpoint_call`, Stufe 1), **ohne** Nachruf-Stufe (seit v0.9.331). Nur mit Fähigkeitsmerkmal `can_repeat_prep_call` im `TlState`; ein älterer Host kennt die Aktion nicht. |
| `announce_court_call` | `side` — **optional**, fehlend = beide | Erneuter Aufruf am Feld. Die Stufe gehört dem **Feld**, nicht der Partei — alle Geräte zeigen dieselbe Zahl. |

`announce_court_call.side` ist die **einzige** Ausnahme von der Regel „kein
Feld in `TlAction` trägt `#[serde(default)]`" — hier ist `None` die
*neutralere* Variante (beide Parteien, das Verhalten von jeher), nicht die
weitreichendere. Dieselbe Abwägung wie bei `EnterResult.winner` und
`CallPreparation.location_id`. Ein älterer Browser, der das Feld nicht
kennt, ruft damit unverändert beide Parteien.

**Stufenzählung am Feld:** Die Stufe steigt einmal je *Aufruf-Runde*. Ruft
die Turnierleitung nacheinander Partei A und Partei B, ist das **eine**
Runde (beide hören „Zweiter Aufruf"); erst ein Aufruf an eine bereits
gerufene Partei eröffnet die nächste Stufe. Der Host merkt sich dazu je
Feld, welche Parteien auf der aktuellen Stufe schon dran waren
(`TabletState::call_stages`). Ein Aufruf aus der Desktop-Oberfläche
(`reached_court_call`) gilt immer beiden und schließt die Runde ab.

### Schiedsrichter (Spec schiedsrichter-management)

Neue `TlAction`-Varianten (geschlossener Satz, ADR 0011):
`official_assign`, `official_clear`, `official_pause`, `official_reorder`,
`official_set_club`, `official_blocklist_set`, `officials_court_toggle`,
`announce_officials`. Die Rolle reist als `"sr"`/`"ar"`
(`relay_proto::TlOfficialRole`).

Seit v0.9.365 (Spec `sr-rotationsmodus`, ADR 0079):
- `official_reorder` trägt optional `list: "ar"` (AR-Liste in Modus
  „getrennt"); fehlt das Feld, gilt die Hauptliste.
- Neu `official_set_role { officialId, role }` mit `role` = `"beides"`,
  `"sr"` oder `"ar"`; der Host prüft den Wert.
- `TlState` trägt `officials_ar_order` (nur IDs, nur in Modus „getrennt"),
  `officials_rotation_mode`, `can_set_official_role` und je Official
  `role`/`next_role`/`next_rank` — alles optional, opak durchgereicht.
- **Deploy-Reihenfolge:** Der Relay liest `TlAction` typisiert. Ein alter
  Relay verwirft `list` still (der Zug landete in der SR-Liste) und lehnt
  `official_set_role` ab. Der Relay deployt deshalb beim Merge, **vor**
  dem Client-Release. Die Seite zeigt Rollenwahl und AR-Liste nur, wenn der
  Host `can_set_official_role` bzw. `officials_ar_order` liefert.

Dazu ein Frame-Paar für den **gezielten** Detail-Abruf, Muster Punktverlauf:
`RelayFrame::OfficialDetailRequest { req_id, official_id }` →
`HostFrame::OfficialDetail { req_id, json }`, ausgeliefert über
`GET /tl/api/officials/{official_id}` (Geräte-Token). Der Relay hält diese
Antwort **nie** vor — Sperrlisten sind Personendaten; er korreliert nur über
`req_id` und lässt offene Anfragen beim Host-Abriss fallen (wie beim
Punktverlauf), statt sie leer zu beantworten.

Der Broadcast-`TlState` trägt dagegen nur: `officials_managed`, die Liste
`officials` (Name, Pause, Dienst-Feld, Einsatz-Zähler) und je Feld
`sr`/`ar`/`official_warn` samt den drei Feld-Schaltern. Zwei Wächter-Tests in
`tl.rs` halten das durchsetzbar fest.

**Reihenfolge beim Ausrollen:** Ein alter Relay lehnt unbekannte Aktionen ab
— erst Relay deployen, dann den Client veröffentlichen.

**Einteilung an der Warteliste** (seit v0.9.354, Spec
[tl-warteliste-schiedsrichter](features/tl-warteliste-schiedsrichter.md),
ADR 0078). Kein neuer Wire-Typ: `official_assign`/`official_clear` mit
`courtId: 0` gab es schon. Neu sind nur additive `TlState`-Felder, alle
`serde(default)` und leer **nicht** serialisiert (64-KiB-Fenster):

- `can_assign_waiting_officials` (Bool): Der Host prüft die Einteilung und
  reserviert die Person. Die Seite zeigt den Menüpunkt nur damit.
- `TlMatch.sr_id`/`ar_id` (0 fehlt): IDs der angezeigten Besetzung für die
  Vorauswahl im Dialog.
- `TlOpenMatch.sr`/`ar`/`sr_id`/`ar_id`/`official_warn`: dieselbe Besetzung
  an offenen Paarungen. Nur Schiedsrichter-Namen, keine Lizenznummer.
- `official_warn` an Warteliste und offener Liste: bleibt **Kategorie-Text**,
  jetzt in fester Reihenfolge `<Verein|Person> · SR doppelt · AR in Pause`
  (nur vorhandene Teile). Die Feld-Kachel trägt weiter nur die Kategorie.
- `TlOfficial.reserved` (Bool) und `reserved_match_num` (Spielnummer, fehlt
  ohne Nummer).

Der Host lehnt `official_assign`/`official_clear` als `NotAllowed` mit Text
ab, wenn das Spiel unbekannt oder beendet ist, die Person nicht in der
BTP-Liste steht oder dieselbe Person SR und AR wäre. Über die Cloud kann jede
beliebige `matchId` ankommen, deshalb prüft der Host vor dem Speichern.

**Deploy-Reihenfolge:** Der Relay braucht keinen Code, deployt aber beim Merge
das neue `tl.html` mit dem Menüpunkt (v0.9.355). An einem älteren Host fehlt das Flag, der
Menüpunkt bleibt verborgen. Sonst landete die Einteilung ungeprüft und ohne
Reservierung im alten Host.

### Spielersuche (seit v0.9.368, Spec [tl-spielersuche](features/tl-spielersuche.md), ADR [0081](adr/0081-tl-spielersuche-am-turnier-pc.md))

Die Suche läuft **am Turnier-PC** über den ganzen BTP-Bestand; der Relay ist
nur Briefträger. Frame-Paar nach dem Muster der Schiedsrichter-Details:
`RelayFrame::TlSearchRequest { reqId, query, candidates }` →
`HostFrame::TlSearchResult { reqId, json }` (opakes JSON, leer = Fehler).

- Route `POST /tl/api/search` (LAN und Relay gleich), Geräte-Token im
  `Authorization`-Header, Rumpf `{"q":"…","candidates":true}` höchstens
  1 KiB (sonst 413). Der Begriff steht **nie in der URL** — er ist ein
  Personenname und gehörte sonst ins nginx-Access-Log. Weder Relay noch
  Turnier-PC protokollieren ihn.
- Der Relay prüft grob vor (2–64 Zeichen, höchstens 6 Wörter, Konstanten
  `TL_SEARCH_*` in `relay-proto`) und schickt Ungültiges gar nicht erst zum
  Host (400). Eigene Pending-Map `search_pending` (Deckel
  `MAX_PENDING_PER_NS`, sonst 429), beim Host-Abriss verworfen.
- Ungültige Begriffe, die erst nach der Faltung am Host auffallen (z. B.
  „ue" = ein Zeichen), kommen als `{"error":"invalid"}` zurück → 400; laufen
  am Host schon zwei Suchen (`MAX_GLEICHZEITIG`, LAN und Cloud zusammen),
  als `{"error":"busy"}` → 429. Am LAN antwortet der Host direkt so.
- Antwort ≤ `MAX_TL_SEARCH_LEN` (256 KiB); der Host kürzt selbst darunter
  (Deckel 50 Treffer je Gruppe, notfalls halbiert), Größeres macht der Relay
  zu 503. `Cache-Control: no-store`.
- Fähigkeit: `TlState.can_search`. Die Seite zeigt das Suchfeld nur damit —
  ein älterer Turnier-PC verwirft den unbekannten Frame still, und jede
  Suche hinge sonst 20 s am `TL_TIMEOUT`.
- Profil-Schalter `hideSearch` in `TlDisplaySettingsWire` (invertiert,
  fehlt = sichtbar). Nur Anzeige, kein Zugriffsschutz.
- Positivliste der Antwort und Datenschutz: siehe Spec, Wächter
  `search_answer_*` in `tablet/suche_tests.rs`. Neu gegenüber `TlState`:
  SR-/AR-Namen an beendeten Spielen (nur mit Schiedsrichter-Betrieb).

**Deploy-Reihenfolge:** Der Relay deployt beim Merge und kennt die Route
dann; der Turnier-PC meldet `can_search` erst ab v0.9.368. Bis dahin bleibt
das Suchfeld verborgen.

### Setzplätze (seit v0.9.298, Spec [setzplaetze-anzeigen](features/setzplaetze-anzeigen.md), ADR [0059](adr/0059-setzplatz-als-text-aus-dem-parser.md))

Die Setzung reist als **fertiger Text** ohne Klammern („1", „3/4") und nur,
wenn sie gesetzt ist (`skip_serializing_if`); ältere Gegenstellen lesen ein
fehlendes Feld als „ungesetzt":

- `MatchBrief.seedA`/`seedB` und `PreparedMatch.seedA`/`seedB` (Host → Relay)
- `MonitorMatch.team1Seed`/`team2Seed` (Court-Monitor)
- in der Cloud-Übersicht (`overview_health`) und der Vorbereitungsliste
  **ausdrücklich** als `team1_seed`/`team2_seed` — gleicher Schlüssel wie die
  LAN-`/health`; der Relay baut diese JSONs von Hand, ein neues Feld kommt
  dort nur an, wenn es eingetragen wird.
- im TL-Zustand an `TlMatch`/`TlCourt` (`team1_seed`/`team2_seed`), dazu der
  Profil-Schalter `hideSeeds` (invertiert, fehlt = anzeigen).

**Neusetzung an einem belegten Feld:** Die Setzung ist Teil des
Push-Schlüssels `CourtPushKey` in `relay_client.rs`. Ändert sie sich, geht
`MatchAssigned` mit derselben Match-ID erneut an den Relay — wie bei einem
Wechsel der Schiedsrichter-Besetzung.

### Panel-Profile (Spec [tl-web-panelsystem](features/tl-web-panelsystem.md), ADR [0024](adr/0024-tl-panel-profile-verwaltung-im-web.md)/[0025](adr/0025-tl-panel-profile-transport-persistenz.md))

Benannte Profile bündeln Panel-Sichtbarkeit/-Reihenfolge/-Höhe und die
turnierweiten Anzeige-Schalter an einem Ort, damit ein Tablet im Handbetrieb
und ein Wandmonitor mit demselben Turnier-PC unterschiedlich aussehen
können. **Hybrid-Transport** (ADR 0025), weil `tl_state_route` einen
einzigen, je Namespace gecachten Blob liefert — identisch für jedes
Gerät — und eine per-Gerät unterschiedliche Information (welches Profil ist
meins) sich nicht ohne Weiteres dort hineinschreiben lässt:

- **Katalog** (alle Profile inkl. Inhalt) → eingebettet in `TlState`
  (`profiles: Vec<TlPanelProfileWire>`, `default_profile_id`), Muster
  `layouts`/`TlHallLayout` — geteilt, klein, unkritisch, folgt demselben
  Cache-/ETag-Modell wie der übrige Zustand.
  Ein Profil trägt neben Name, Panel-Liste und Anzeige-Schaltern die
  Layout-Aufteilung: `columns` (1…3), `columnWidths` (relative
  Spaltenbreiten, leer = gleichmäßig) und je Panel `heightFr`, `collapsed`
  und `column` (1-basiert). Alle drei Layout-Felder tragen
  `#[serde(default)]` — dieselbe Abwägung wie bei `TlAuthDevice.profile_id`
  unten: `0`/leer ist die **neutralste** Lesart, nicht die
  weitreichendere. Der Host **reicht sie nur durch**; was `0` bedeutet
  („aus `listPosition` ableiten" bzw. „Spalte 1"), entscheidet
  ausschließlich `tl.html`. Serverseitig begrenzt sind lediglich die
  Längen (`MAX_TL_PROFILE_PANELS`, `MAX_TL_PROFILE_COLUMN_WIDTHS`), damit
  ein einzelner Aufruf den `TlState` nicht über `MAX_TL_STATE_LEN` treiben
  kann (R4). Seit Spec `spielzeiten-prognose` Etappe D trägt
  `TlDisplaySettingsWire` zusätzlich `showCourtRemaining` (seit v0.9.342
  außerdem `showLastPlayed`, Spec `tl-zuletzt-gespielt`, `#[serde(default)]`
  = aus; das Anzeigedatum reist als `TlMatch.team1_last_played_ms`/
  `team2_last_played_ms` im opaken `TlState`-JSON; seit v0.9.411 ebenso
  `showPlannedTimes`, Spec `tl-zeitplan` — die Daten dazu sind
  `TlMatch`/`TlOpenMatch`/`TlFinished.planned_time`,
  `TlFinished.start_delay_mins` und `TlState.today`. **Relay vor Client:**
  Das Relay reicht `TlAction::ProfileSave` typisiert durch; ein Relay ohne
  das Feld verliert das Häkchen beim Speichern)
  (`#[serde(default)]` — alte Browser-Profile ohne das Feld lesen sich als
  „aus"); das zugehörige Anzeigedatum reist als `TlCourt.remaining_min`
  im opaken `TlState`-JSON mit (Serde-Default, alte Gegenstellen
  ignorieren es). Seit 17.08.2026 trägt `TlDisplaySettingsWire` außerdem
  `unlimitedCourtCalls` (inzwischen Container-weites `#[serde(default)]`
  wie beim Config-Zwilling: fehlende Häkchen-Felder lesen sich als „aus").
  Der Turnier-PC zählt Aufruf-Stufen nur dann über 3 hinaus, wenn
  irgendein Profil die Option führt (`tablet/tl.rs`
  `unlimited_court_calls`) — ohne sie hält er den alten 3er-Deckel
  selbst, das Client-Gating ist nicht die einzige Sicherung. Seit
  30.08.2026 trägt `TlDisplaySettingsWire` zusätzlich `hideOpenMatches`
  (Spec `tl-offene-paarungen`) — **invertiert benannt**, damit ein Profil
  aus einem älteren Browser-Stand, das das Feld nicht kennt, auf „offene
  Paarungen anzeigen" steht. Ein neuer Wire-Frame war dafür nicht nötig:
  Die offenen Spiele selbst reisen als `open_queue` im opaken
  `TlState`-JSON (Serde-Default, alte Gegenstellen ignorieren die Liste,
  und eine neue Seite an einem alten Host zeigt schlicht keine offenen
  Spiele). Seit
  v0.9.218 trägt der opake `TlState` außerdem optional `checkin_times`
  samt `checkin_stale`-Marke (Panel „Anfangszeiten": heutiger
  Check-In-Zeitplan je Klasse mit Zählern, ohne Personendaten;
  `#[serde(default)]`, alte Gegenstellen ignorieren es). Die
  Relay-Kürzungsleiter (`state_for_relay`) kennt die Liste: Reichen alle
  Warteliste-Stufen nicht, wird zuletzt das Anfangszeiten-Panel geopfert,
  bevor der Zustand die Relay-Grenze reißen dürfte. Seit v0.9.280 trägt
  `TlDisplaySettingsWire` zusätzlich `showMoveButtons` (vier
  Verschiebe-Knöpfe an jeder Wartelisten-Zeile) — wieder
  `#[serde(default)]`; fehlt es, bleibt der Schalter aus. Ein neuer Host
  serialisiert es in jedem Profil, daran erkennt die Seite, ob sie das
  Häkchen anbieten darf.
- **Individuelle Geräte-Zuordnung** → reitet auf dem bestehenden
  `HostFrame::TlAuth`-Spiegel: `TlAuthDevice.profile_id` (neu, siehe unten).
  Der Relay hält eine zweite Parallel-Map neben `tl_tokens` (Zugang →
  `profile_id`), **strikt Namespace-lokal**, und liefert sie als
  Antwort-Header `X-Tl-Active-Profile` auf **jede** `GET /tl/api/state`-
  Antwort — auch bei `304`, da Header unabhängig vom gecachten Body immer
  gesendet werden und der Body dadurch weiter cachebar bleibt. Fehlt der
  Zugang in der Map (kein Profil zugewiesen), bleibt der Header schlicht
  weg — kein geratener Fallback. Der LAN-Pfad setzt denselben Header direkt
  aus dem authentifizierten `TlDevice` (`tablet::server::tl_state`).
- **Schreiben** (Anlegen/Bearbeiten/Löschen/Wählen/Default) läuft in
  beiden Betriebsarten identisch über vier neue `TlAction`-Varianten,
  einmal geprüft in `tl.rs::execute` (R5):

  | `TlAction` | Zweck |
  |---|---|
  | `profile_save` | Profil anlegen/überschreiben (Upsert nach `id`; leere `id` = neu, der Host vergibt dann eine Kennung). Last-Write-Wins — bewusst **keine** Konfliktprüfung gegen `updated_at_ms`, die Spec verlangt ausdrücklich keine Fehlermeldung bei gleichzeitiger Bearbeitung. Der Host stempelt `updated_at_ms` immer selbst. |
  | `profile_delete` | Profil löschen; Geräte, die es trugen, fallen auf das Standardprofil zurück (leere `profile_id`) — kein Fehlerzustand. |
  | `profile_select` | Für das **aufrufende** Gerät ein Profil wählen — bewusst ohne Geräte-Feld im Payload, das Gerät ist aus der Bearer-Token-Auth bekannt (Sicherheitsgrenze: ein Gerät darf nur sich selbst binden). |
  | `profile_set_default` | Das turnierweite Standardprofil setzen (leer = eingebautes Standardprofil in `tl.html`). |

`TlAuthDevice` trägt dafür ein neues Feld `profile_id: String`, mit
`#[serde(default)]` **auf Feldebene** — eine bewusste, dokumentierte
Ausnahme von der oben stehenden Regel „kein `#[serde(default)]` auf
TL-Feldern": Diese Regel schützt davor, dass ein still ergänzter Wert die
*weitreichendere* oder *ungeprüfte* Variante auslöst (fehlendes `expect`,
fehlendes `tokens`, fehlendes `side`). Hier ist „leer" dagegen die
**neutralste** Lesart — sie bedeutet „Standardprofil", keine erweiterten
Rechte und keine größere Sichtbarkeit. Ein alter Host, der das Feld noch
nicht kennt, sendet es schlicht nicht mit; der Relay bleibt damit
abwärtskompatibel lauffähig (ADR 0025).

**Sicherheitsgrenze, die `security-reviewer` explizit geprüft hat:** Ein
Zugang aus Namespace A darf niemals einen `X-Tl-Active-Profile`-Wert aus
Namespace B bekommen — die Map lebt strikt innerhalb ihres `Namespace`, kein
globaler Zustand, genau wie `tl_tokens`.

Persistenz bewusst **installationsweit** in `AppConfig` (`tl_web.profiles`
+ `tl_web.default_profile_id`), nicht turniergebunden: Profile sind
geräteklassen-/installationsbezogen (welcher Wandmonitor zeigt was), nicht
turnierbezogen. `keep_host_managed_fields` schützt live editierte Profile
vor dem Setup-Assistenten (Muster `devices`); `identity_bundle` strippt den
Profil-**Katalog** NICHT (kein Zugang/Secret, wandert bei PC-Umzug mit wie
`hall_layouts`) — nur `TlDevice.profile_id` verschwindet implizit, weil der
Identitäts-Export die komplette Geräteliste ohnehin leert (ADR 0012).

## Anzeige-Stil der Werbebilder (seit v0.9.247)

`AdUpload` trägt neben `inBar` ein optionales `style`:
`{ bg, fg, showCourt }` — Hintergrundfarbe, dazu kontrastierende Schriftfarbe
und ob die Feldbezeichnung über der Werbung erscheint. `MonitorState` und
`/{ns}/info/ad/state` geben die Stile als `adStyles` **index-parallel zu
`ads`** aus.

Zwei Eigenheiten, die man kennen muss (ADR 0041):

- **Positionell, nicht per Schlüssel.** `AdUpload` führt keinen Bildnamen; der
  Relay löst Bilder über ihre Position auf. Der Stil erbt damit die bekannte
  Index-Verschiebung: Wird ein Bild gelöscht, rücken die Folge-Indizes auf, und
  bis zum nächsten Upload (max. 30 s Fingerabdruck-Takt plus 60 s Bild-Cache)
  kann eine Anzeige ein Motiv mit dem Stil seines Nachbarn zeigen. Im LAN
  entfällt das — dort ist der Dateiname der Schlüssel.
- **`fg` rechnet der Host**, nicht der Relay und nicht die Anzeigeseite. So
  gibt es genau eine Stelle, die den Kontrast bestimmt. Ein Upload ohne `style`
  (ältere App) bedeutet „nichts eingestellt"; die Anzeige fällt dann auf
  Schwarz ohne Feldbezeichnung zurück, also auf das Bild von vor dem Feature.

Der Fingerabdruck in `relay_client.rs` führt Farbe und Häkchen mit
(`ad_abdruck_zeile`) — ohne das löste eine reine Farbänderung nie einen Upload
aus, weil sie weder Datei noch Verzeichnis anfasst.

## Stream-Werbung: `inStream` und `?zweck=stream` (seit v0.9.395)

Spec [stream-werbung](features/stream-werbung.md),
ADR [0092](adr/0092-stream-werbung-markierung-und-abriss.md).

- **Wire:** `AdUpload.inStream` (bool, `serde default` = aus), positionell wie
  `inBar`. Der Host setzt es aus `court-ad-stream.json` im Werbe-Verzeichnis.
  Ein Upload einer älteren App liest sich als `false` — die Stream-Ebene bleibt
  leer. Ein älterer Relay verwirft das Feld.
- **Fingerabdruck:** `ad_abdruck_zeile` führt das Flag
  (`|name:len:mtime:in_bar:in_stream:bg:zeigt_feld`). Ohne das käme ein reines
  Umhaken nie in die Cloud.
- **Zustand:** `/{ns}/info/ad/state` liefert zusätzlich `streamAds` (Indizes der
  angehakten Bilder) und `streamVer` (je Eintrag die Inhaltsmarke des Bildes,
  `AdImage.etag` ohne Anführungszeichen). Die Stream-Seite hängt sie als `?v=`
  an `/{ns}/ads/{i}` — so liefert der 60-s-Bild-Cache nach einem Löschen mit
  nachrückenden Indizes kein falsches Motiv.
- **503 statt leer, nur mit `?zweck=stream`:**
  - Anzeige-Lage gesperrt (Start-Karenz, Host weg, ADR 0088) → `{"grund":"host_weg"}`;
  - kein Monitor-Upload (Relay-Neustart, Daten-Frist ADR 0089, Host noch nicht
    hochgeladen) → `{"grund":"keine_daten"}`;
  - kein Host und keine Bilder (vom Budget freigegeben) → `keine_daten`.

  Ohne `zweck` antwortet die Route unverändert mit 200 — `ad.html` und die
  Sponsor-Leiste merken nichts. Die Stream-Seite wertet nur „200 mit Liste
  `streamAds`" aus, alles andere heißt „stehen lassen".
- **LAN-Gegenstück:** `/info/ad/state` liefert `streamAds` als Dateinamen (ohne
  `streamVer`, die Namen sind eindeutig) und antwortet mit `?zweck=stream` mit
  503, wenn die Bildliste leer ist, obwohl ein angehaktes Bild auf der Platte
  liegt (Lesefehler im Werbe-Verzeichnis). Verwaiste Markierungen ohne Datei
  zählen als leere Auswahl (200).

## Monitor-Stile und Nation/Verein (seit v0.9.327)

Spec [monitor-stile-und-stream](features/monitor-stile-und-stream.md),
ADR [0068](adr/0068-monitor-stil-als-attribut-am-feld-ziel.md) und
[0070](adr/0070-nation-verein-auf-anzeigen-serverseitig.md).

- **Stil:** `MonitorTarget::Court` trägt optional `stil`
  (`international|riesen|spieler|hell|stand`; Standard wird weggelassen →
  `{"kind":"court","court_id":5}` bleibt bytegleich). Der Geräte-Arm von
  `/{ns}/monitor/state` übernimmt ihn als `MonitorState.stil`; die feste Route
  `/{ns}/court/{id}/state` liefert nie einen. Ein unbekannter Wert liest sich
  als `Unbekannt` (`serde(other)`) und wirkt wie Standard — der Upload bleibt
  lesbar, kein 422, kein Einfrieren wie bei der Tafel (ADR 0055).
- **Nation/Verein:** `MonitorConfig.showNations` (Standard an) und
  `showClubs` (Standard aus) — der Host leitet sie aus den Plätzen am Namen
  ab und schickt die Plätze selbst als `nameDavor`/`nameDarunter` mit (der
  Relay reicht sie nur durch). Der Relay filtert in `monitor_player` und in
  `overview_health` nach den Schaltern; ohne Host-Upload gilt der Standard.
  `MonitorPlayer.club` ist neu und fehlt, solange der Schalter aus ist.
- **Vereinslogo:** `/{ns}/info/club-logo?name=` leitet relativ auf den
  badhub-Resolver weiter (Name kodiert, auf 120 Zeichen gekappt, fester Pfad
  → kein offener Redirect). So nutzen die Seiten in LAN und Cloud dieselbe
  Adresse `BASE + "info/club-logo?name="`.

**Deploy-Reihenfolge und Rückfall:** Der Relay geht mit dem Merge automatisch
live, der App-Tag folgt. Ein **alter Host** am neuen Relay bedeutet Standard-
Stil, Nation an, Verein aus — also das bisherige Bild. Ein **alter Relay** am
neuen Host (Relay-Rollback) ignoriert `stil` (Cloud zeigt Standard) **und
`showNations`**: Nutzt kein Platz am Namen die Nation, stehen in der Cloud
dann trotzdem wieder Flaggen. Das ist die einzige Datenschutz-Folge eines
Rollbacks; der Verein bleibt aus, weil der alte Relay `club` gar nicht kennt.
Ein alter Relay verliert außerdem `nameDavor`/`nameDarunter`; die Seite leitet
die Plätze dann aus den Schaltern ab (Flagge davor bzw. Vereinsname darunter)
— nur bis zum nächsten automatischen Relay-Deploy.

## Stream-Einblendung (seit v0.9.328)

ADR [0069](adr/0069-stream-einblendung-eigene-seite.md). `/{ns}/stream`
liefert `stream.html`; die Seite holt je Feld (höchstens drei) den festen
Zustand `/{ns}/court/{id}/state?zweck=stream` und hört auf `/{ns}/monitor-ws`
ohne Feldfilter. `zweck=stream` lässt den Relay Nation/Verein nach
`MonitorConfig.streamNations`/`streamClubs` filtern (`build_stream_state`) statt
nach den Monitor-Schaltern. Die feste Zustandsroute antwortet seitdem wie die Gerätestrecke mit
**`ETag` und 304** (`monitor_state_response`) — vorher nur `no-store`.

Rückfall: Ein Relay-Rollback lässt `/{ns}/stream` mit 404 antworten; OBS zeigt
dann eine **nicht durchsichtige** Fehlerseite im Bild. Die feste Route fällt
auf Antworten ohne Marke zurück, was nur Last kostet.

## Host-Schleife: lesen, schreiben, arbeiten getrennt (seit v0.9.379)

Spec [relay-haerten](features/relay-haerten.md), PR 1 ·
ADR [0091](adr/0091-ergebnisse-im-host-ein-arbeiter-je-feld.md).

Der Turnier-PC hält **eine** Verbindung zum Relay für alle Cloud-Tablets und
-Monitore. Der Relay verwirft sie, wenn 15 s lang nichts von ihr kommt. Bis
v0.9.378 wartete die Schleife dieser Verbindung an drei Stellen selbst: auf
den BTP-Write eines Ergebnisses, auf den Werbe-Upload und auf den Abgleich der
Monitor-Steuerung. Solange kam nichts heraus — auch kein Pong. Ein träges BTP
oder eine laufende TP-Kopie (Schreib-Gate, bis etwa 20 s) trennte so die ganze
Cloud-Halle.

Seit v0.9.379 (`tablet/relay_client.rs`, `sitzung_fahren`):

| Teil | Aufgabe |
|---|---|
| **Schleife** | liest, verteilt, tickt. Wartet auf nichts Langes. Im `select!` wird zuerst gelesen — liegt ein Lebenszeichen bereit, urteilt der Idle-Wächter nicht davor. |
| **Schreiber-Task** | besitzt die Sende-Hälfte, 20 s Frist je Nachricht. Scheitert oder hängt das Senden, endet die Sitzung und der Turnier-PC verbindet neu. |
| **Ergebnis-Arbeiter je Feld** | schreibt nach BTP (`process_result`, unverändert — R5) und quittiert. |
| **Upload-Task, Steuerungs-Task** | je höchstens einer zugleich („läuft schon"-Wächter). |
| **Zettel, Spielersuche** | `spawn_blocking`. |

**Reihenfolge der Ergebnisse.** Ergebnisse *eines* Felds laufen strikt in der
Reihenfolge ihres Eingangs, verschiedene Felder nebeneinander. Der 8-s-Retry
des Tablets reiht sich hinter den ersten Versuch und trifft danach die
Idempotenz (ADR 0018) — es gibt keinen zweiten BTP-Write. Je Feld warten
höchstens vier Ergebnisse; ein weiteres bekommt sofort eine wiederholbare
Absage („Der Turnier-PC verarbeitet gerade ein Ergebnis für dieses Feld").
Ergebnisse für eine CourtID, die der Turnier-PC nicht kennt, teilen sich einen
Sammel-Arbeiter: Die CourtID kommt vom Tablet, und ein Fremder soll damit
nicht beliebig viele Tasks anlegen können.

**Quittung nur an die eigene Sitzung.** Die Arbeiter leben über die
Sitzungen hinweg: Reißt die Verbindung mitten im Write ab, läuft er zu Ende.
Die Quittung geht aber nur an die Sitzung, aus der der Auftrag kam — gibt es
sie nicht mehr, verpufft sie, und das Tablet wiederholt. Über eine neue
Sitzung ginge sie nie: Der Relay zählt `req_id` je Namespace ab 1, nach
seinem Neustart könnte eine alte Nummer ein fremdes Ergebnis quittieren.

**Was sich an der Reihenfolge ändert.** Solange der Write inline lief, konnte
der 2-s-Takt erst danach ein `MatchCleared` oder das Folgespiel melden. Jetzt
kann beides **vor** der Quittung beim Tablet ankommen — wie im Hallennetz schon
immer (dort läuft der Write im HTTP-Handler, die Zuweisung über den Socket).
Das Tablet verträgt das seit v0.9.376 (geparktes Ergebnis, Spec
`tablet-stand-robust`). Ebenso kann ein Punkt desselben Spiels, der während
des Writes eintrifft, noch aufgenommen werden; vorher lag er hinter der
Finalisierung. Wartet der Relay nicht mehr auf eine Quittung (8 s), läuft ein
schon eingereihter Retry trotzdem noch und endet an der Idempotenz.

**Lebensdauer.** Arbeiter und Hintergrund-Tasks (Upload, Abgleich) gehören
einem Lauf der Übertragung. Sie überleben Reconnects, enden aber mit dem
Stoppen oder dem Speichern der Einstellungen. Stirbt ein Arbeiter an einer
Panik, wird er beim nächsten Ergebnis des Felds ersetzt. Zu Beginn jeder
Sitzung vergisst der Upload seine Pause — ein frisch gestarteter Relay hat
keine Bilder mehr.

**Werbe-Upload.** 120 s Frist statt der 15 s des gemeinsamen HTTP-Clients;
nach einem Fehlschlag wachsende Pausen (30 s, 60 s, 120 s, 240 s, dann 5 min).
Der Fingerabdruck „schon hochgeladen" gilt weiter je Sitzung.

Nicht gelöst: Ein sehr großer Zettel-Abruf über einen schmalen Uplink kann
das Senden länger als 15 s belegen (Roadmap).

## Verbindungsaufbau: Frist und Backoff (seit v0.9.380)

Spec [relay-haerten](features/relay-haerten.md), PR 2. Gemeinsamer Helfer
`tablet/ws_verbindung.rs` für den Host (`relay_client.rs`) und den Träger der
fernen Halle (`carrier.rs`).

**Frist:** 10 s für DNS, TCP, TLS und Upgrade zusammen. Vorher hing
`connect_async` hinter einem Captive Portal unbegrenzt.

**Backoff** (reiner Automat, `Backoff::pause_nach`):

| Ausgang des Versuchs | Nächste Pause |
|---|---|
| Sitzung stand mindestens 30 s | zurück auf 1 s |
| Aufbau gescheitert (Netz, Frist, 502) | verdoppeln bis 30 s |
| Abriss binnen 30 s | verdoppeln bis 30 s |
| Close mit Code 4409 („Host belegt") | verdoppeln, höchstens 5 s |
| sauberer Close binnen 30 s ohne Grund | verdoppeln, höchstens 5 s |
| Abriss in den ersten 5 s nach dem Aufbau | wie Close ohne Grund — der Close-Frame einer Abweisung kann unterwegs verloren gehen |

Jede Pause wird um ± 20 % gestreut, damit nach einem Relay-Neustart nicht
alle Turnier-PCs im selben Augenblick kommen.

Bis v0.9.379 sprang die Pause schon nach dem bloßen Handshake auf 1 s zurück.
Schloss der Relay gleich wieder (belegter Platz), entstand ein
Dauer-Reconnect im Sekundentakt. Die kurze Obergrenze bei Abweisung ist
Absicht: Nach einem Netzwechsel ist die eigene tote Verbindung bis zu 15 s
lang der „andere" Host; wer dicht nachfragt, übernimmt nach rund 20 s.

Den Close-Code 4409 sendet der Relay erst mit PR 7 der Spec. Bis dahin (und
gegenüber einem älteren Relay) schließt er ohne Grund — die letzte Zeile der
Tabelle behandelt das gleich.

**Träger der fernen Halle:** dieselbe Frist, derselbe Automat. „Stabil" zählt
dort erst ab dem `Ready` des Relays. Zusätzlich: 20 s Frist je gesendeter
Nachricht; Frames ohne bereiten Träger und Nutzlast über 160 KiB werden schon
im Slave verworfen (Letzteres einmal im Log gemeldet); Reste der alten Sitzung
im Ausgang werden vor dem `Hello` geleert.

## Trägerverbindung der fernen Halle (`/{ns}/carrier-ws`)

Vorbereitet für die **Transport-Bündelung** der fernen Halle
([Spec](features/ferne-halle-transport-buendelung.md),
[ADR 0048](adr/0048-substrom-adressierung-traeger.md)). Der Slave-PC bündelt
die WebSockets seiner Geräte über **eine** Verbindung; jedes Gerät ist darin
ein **Substrom**.

> **Stand: beide Seiten stehen.** Der Slave nutzt die Route nur bei
> ausdrücklichem Opt-in (`slave_mux.enabled`, Vorgabe **aus**) — ohne das
> ändert sich im Betrieb nichts.

**Wire-Form** (`relay-proto`): Der Slave sendet `CarrierMsg` —
`hello` (Fassung aushandeln), `streamOpen` (Gerät verbunden; `kind` ist
`tablet` oder `monitor`, bei Anzeigen optional mit `court`), `frame`
(Fachframe 1:1 wie vom Gerät) und `streamClose`. Der Relay antwortet mit
`CarrierServerMsg` — `ready`, `unsupported`, `frame`, `streamClose`.
Deckel: `MAX_STREAMS_PER_CARRIER` = 64, `MAX_CARRIER_PAYLOAD_LEN` = 160 KB (mit
Kopfraum ueber `MAX_STATE_LEN`, weil hier die UMHUELLTE Form gemessen wird).

**Der Kern: ein eigener Sende-Kanal je Substrom.** Der Relay unterscheidet
Geräte über `Tx::same_channel()` — daran hängen `is_holder` (und damit R4,
„ein aktives Tablet je Court"), `detach_tablet`, `unsubscribe_monitor` und
sämtliche Deckel. Weil `carrier_conn` je Substrom einen eigenen Kanal anlegt
und damit die **unveränderte** Sitzungslogik aufruft, arbeiten diese Stellen
weiter, als hinge jedes Gerät an seiner eigenen Leitung. Ein gemeinsamer Kanal
hätte dagegen jeden Substrom zum Halter **jedes** Felds gemacht — ein Ergebnis
liefe ins fremde Feld, ohne dass ein Test rot würde.

**Eigene Rolle neben dem Host.** Der Träger beansprucht den `host`-Slot nie;
`try_claim_host` und `release_host_slot` bleiben unberührt, R4 („genau ein
Host je Namespace") gilt wörtlich weiter.

**Was NICHT durch den Träger läuft:** HTTP. Über die Tablet-Strecke gehen
höchstens 64 KB, die Werbebilder (12 MB) und das Turnierlogo (2 MB) dagegen
als HTTP-Uploads — zöge man sie mit hinein, teilten sie sich die Leitung mit
den Punkt-Frames. Ebenso außen vor bleiben **TL-Web-Geräte**, damit keine
Schreib-Token durch den Träger reisen.

**Liveness: der Slave zuerst, der Relay als Rückfallebene.** Stirbt ein
einzelnes Gerät, meldet es der Slave mit `streamClose` — er hält die echte
Verbindung dorthin und sieht den Abriss schneller als die 15 s
`TABLET_STALE`. Versagt er, greift der Relay: Ein Tablet-Substrom, der länger
als `TABLET_STALE` kein Frame geschickt hat, wird abgeräumt und sein
Court-Slot freigegeben. Ohne diese Rückfallebene hielte ein totes Tablet
seinen Court, solange der Träger lebt — und ein **Ersatzgerät** am selben Feld
bekäme nur `CourtOccupied` und müsste von Hand übernehmen. Bricht der Träger
selbst weg, räumt der Relay **alle** seine Substrome ab.

**Anzeigen bekommen ihren Herzschlag.** `overview.html` und `monitor.html`
werten aus, ob binnen `MONITOR_HEARTBEAT_STALE_MS` (25 s) ein Server-Frame
kam, und schalten sonst auf den langsamen Poll zurück. Auf einem ruhigen Feld
entsteht minutenlang kein Nudge — deshalb sendet der Träger denselben
sichtbaren Herzschlag je Anzeige-Substrom, den `monitor_conn` an seinem
eigenen Socket schickt. Anzeige-Substrome sind umgekehrt von der
Stale-Räumung ausgenommen: Sie senden nichts, ihr Schweigen ist der
Normalfall.

**Ein geschlossener Substrom-Kanal wird gemeldet.** `namespace_aufraeumen`
verabschiedet die Anzeigen mit einem `Close`, damit sie sich neu verbinden,
sobald der Host zurück ist. Der Träger reicht das als `streamClose` an den
Slave weiter, statt es zu verschlucken — sonst bliebe der Substrom mit totem
Abo stehen und bekäme nie wieder einen Anstoß.

**Fassungs-Aushandlung.** Passt `hello.proto` nicht, antwortet der Relay
`unsupported` und schließt. Der Slave fällt dann auf die heutigen
Weiterleitungen zurück — ohne das machte ein neuer Slave gegen einen älteren
Relay die ganze Halle blind.

## Ein Feld je Tablet-Verbindung, Halterschaft, Feld-Deckel (seit v0.9.384)

Spec [relay-haerten](features/relay-haerten.md), PR 3 (`relay/src/main.rs`,
`TabletSitzung`).

**`identify` auf einer Verbindung:**

| Lage | Verhalten |
|---|---|
| noch kein Feld | Anmeldung wie bisher (frei → zählt; belegt → wartet auf „Übernehmen") |
| dasselbe Feld, die Verbindung hält es | nichts — kein `session_superseded`, kein zweites `TabletConnected` an den Host |
| dasselbe Feld, die Verbindung hält es nicht | normaler Versuch (der 4-s-Retry der Seite bei „belegt") |
| ein **anderes** Feld | verworfen, einmal je Verbindung im Relay-Log; die Verbindung bleibt |

Ein Feldwechsel ist immer ein Neuladen der Seite mit neuer Verbindung; eine
echte Tablet-Seite trifft die letzte Zeile nie. Bis v0.9.383 belegte jedes
weitere `identify` ein Feld mehr, beim Trennen wurde nur das letzte gelöst —
die übrigen blieben als tote Slots stehen und hielten den Namespace bis zum
Relay-Neustart.

**Halter ist, wer in der Tablet-Liste des Felds steht** (`is_holder`). Den
Merker „diese Sitzung zählt" gibt es nicht mehr: Er blieb wahr, wenn das
Tablet verdrängt wurde — ein späteres „Übernehmen" wurde dann still
verworfen, die Seite zählte weiter, und der Relay warf ihre Punkte weg.
`take_over` gilt jetzt, wenn die Verbindung ein Feld hat und es gerade nicht
hält. Alle Tablet-Frames — auch der Akkustand — gehen nur vom Halter weiter.

**Feld-Schlüssel:**

- Ein Feld ist *host-bekannt*, wenn es in der Feldliste, einer Zuweisung oder
  den Feldnamen des Hosts steht.
- Geht ein Tablet, räumt der Relay für ein **host-unbekanntes** Feld Stand,
  Sätze, Monitor-Folge und Vor-Match-Fenster. Für ein host-bekanntes Feld
  bleibt alles liegen: Der gesicherte Stand gehört dem Ersatzgerät.
- `MAX_FELD_SCHLUESSEL_JE_NS` = 128 verschiedene Felder je Namespace, über
  alle Feld-Karten. Ein Host-Frame (`MatchAssigned`, `MatchCleared`,
  `ScoreUpdate`) oder ein Tablet, das ein weiteres Feld anlegen würde, wird
  verworfen und einmal je Host-Sitzung gemeldet; die Host-Verbindung bleibt.
  Der Deckel hängt nicht an der Feldliste (die ist nach einem Relay-Neustart
  leer).
- `take_over` legt keinen Namespace mehr ohne Prüfung an; die Geräte-Kennung
  wird auf 128 Zeichen gekürzt. Wird eine Übernahme abgewiesen (ein Deckel
  greift), schließt der Relay die Verbindung — die Seite hielte sich sonst
  für den Halter und zählte ins Leere.
- Mit jeder neuen Feldliste (`Courts`) verwirft der Relay Feldnamen und
  Hallen von Feldern, die weder in der Liste stehen noch ein Spiel oder ein
  Tablet haben. Sonst zählten die Altlasten eines Turnier-PCs, der über
  mehrere Turniere verbunden bleibt, auf den Deckel.
- Die Feldliste selbst darf länger sein (`MAX_COURTS_PER_NS` = 512): Sie ist
  ein Menü, kein Zustand je Feld; ihre Felder gelten immer als geführt.

Bewusst in Kauf genommen: Zählt ein Cloud-Tablet nach einem Relay-Neustart
**ohne Host** und fällt dann aus, bekommt das Ersatzgerät vom Relay keinen
Stand. Sobald der Host zurück ist, greifen Host-Sätze und Rückspiegel.

## Deckel und Mengen-Takt (seit v0.9.385)

Spec [relay-haerten](features/relay-haerten.md), PR 4 ·
ADR [0090](adr/0090-ueberlast-am-relay-und-mindestschutz.md). Zwei Arten:

- **Fachliche Deckel** kürzen oder verwerfen und melden einmal (je
  Tablet-Sitzung bzw. je Host-Sitzung) — **die Verbindung bleibt**.
- **Transport-Deckel** sind reiner Speicherschutz am WebSocket und
  **trennen**. Sie liegen weit über jedem echten Frame. Bis v0.9.384 galt der
  Standard der Bibliothek: 64 MiB je Nachricht.

**Transport-Deckel** (`max_message_size`/`max_frame_size` am Upgrade):

| Verbindung | Deckel | Warum so |
|---|---|---|
| Tablet (`/{ns}/ws`) | 1 MiB | größter zulässiger Frame: 64 KiB Zustand. Auch der Zustand einer alten Format-1-Seite (bis 590 KB) bleibt darunter — er wird fachlich verworfen, ohne die Verbindung zu kosten |
| Monitor (`/{ns}/monitor-ws`), TL (`/tl-ws`) | 64 KiB | die Seiten senden nichts bzw. einen Zugangs-Frame |
| Träger (`/{ns}/carrier-ws`) | 4 MiB | ein alter 590-KB-Zustand wird im Umschlag zweimal escaped (rund 1 MiB) |
| Host (`/{ns}/host-ws`) | 2 × Zettel-Deckel + 64 KiB ≈ 10 MiB | der Relay verwirft einen zu großen Zettel erst **nach** dem Empfang; der Deckel muss den größten zulässigen Zettel samt JSON-Escaping fassen, sonst würfe ein Sammeldruck den Turnier-PC ab |

**Fachliche Deckel:**

| Frame / Route | Deckel | Verhalten |
|---|---|---|
| Tablet `score_update` | 10 Sätze | Frame verworfen |
| Tablet `identify`/`take_over` | Geräte-Kennung 128 Zeichen | gekürzt |
| Tablet `state_sync` | 64 KiB | verworfen (seit v0.9.374) |
| Host `MatchAssigned` | Spiel-Beschreibung 16 KiB · Feldname/Halle 128 Zeichen · 10 Host-Sätze | Beschreibung zu groß: Frame verworfen · Texte gekürzt · zu viele Sätze: ohne Sätze durchgereicht |
| Host `MatchCleared` | Feldname/Halle 128 Zeichen | gekürzt; das Räumen wirkt immer |
| Host `Courts` | 512 Einträge · Name/Halle 128 · Farbe 16 · Sprachdienst-Block 8 KiB | gekürzt; ein zu großer Block wird nicht übernommen |
| Host `Prepared` | 200 Einträge · 256 KiB | Liste geleert (ehrliche Fehlanzeige) |
| Host `TlAuth` | Kennung/Zugang/Profil 256 Zeichen | Eintrag mit überlangem Zugang ausgelassen, Kennung/Profil gekürzt — die übrige Liste gilt (ein Widerruf im selben Push greift) |
| Host `TlState` | 64 KiB (`MAX_TL_STATE_LEN`) | wie bisher |
| `POST /{ns}/monitor` | Turniername 256 Zeichen | gekürzt |
| `POST /{ns}/monitor/control` | 256 Einträge je Karte · Kennung 64 Zeichen | `413` |

Ein echter Brief (Doppel mit langen Namen, Schiedsrichter, Aufschlagrichter)
bleibt unter 2 KiB.

**Mengen-Takt je Tablet-Sitzung** (und je Substrom des Trägers),
Eimer-Verfahren:

| Frames | Schwung | Nachwuchs |
|---|---|---|
| Punkt, Ballwechsel, Akku, Ruf, Einzel-Ereignis | 60 | 20 je Sekunde |
| `rally_sync` | 30 | 2 je Sekunde |
| `match_event_sync` | 30 | 2 je Sekunde |
| `state_sync` | 30 | 3 je Sekunde |
| Weitergabe von `state_sync` an den Host (je Feld) | 6 | 2 je Sekunde |

Ein zählendes Tablet sendet höchstens rund sechs kleine Frames je Sekunde.
Die kompletten Abgleiche und der Zustand haben je einen eigenen, bewusst
großen Takt: Jedes „zurück" am Tablet schickt alle drei, ohne Punkt-Sperre
(250 ms Entprellung), und es gibt 50 Rücknahme-Schritte. Ein verworfener
`rally_sync` wird nie wiederholt — der Host behielte zurückgenommene
Ballwechsel. 30 auf einmal plus Nachwuchs tragen alle 50 Schritte in
schnellster Folge, auch direkt nach einem Wiederverbinden (das selbst drei
Abgleich-Paare schickt). Was über dem Takt liegt, wird verworfen und einmal
je Sitzung gemeldet. `identify`, `take_over` und `ping` zählen nicht.

Der Takt schützt vor einem fehlerhaften Gerät, nicht vor einem Angreifer:
Eine neue Verbindung beginnt mit vollem Kontingent. Die feste Grenze bleibt
die Zahl der Halter (64 je Namespace).

## Ohne Turnier-PC: 20 s Karenz, dann 503 (seit v0.9.388)

Spec [relay-haerten](features/relay-haerten.md), PR 7 ·
ADR [0088](adr/0088-anzeigen-bei-host-verlust-karenz-dann-503.md).

Bis v0.9.387 zeigte eine Cloud-Anzeige ohne Turnier-PC entweder den
eingefrorenen Stand (solange ein Tablet den Namespace hielt) oder eine leere
Leerlauf-Seite. Jetzt führt jeder Namespace **eine Uhr** (`host_weg_seit`):

- gesetzt beim Freigeben des Host-Slots, und zwar auf das **letzte
  Lebenszeichen** — ein stumm gestorbener Turnier-PC ist rund 20 s nach
  seinem letzten Lebenszeichen gesperrt, nicht nach 15 + 20 s;
- gelöscht, sobald ein Turnier-PC den Slot übernimmt;
- ein von einem Tablet angelegter Namespace beginnt mit „weg seit jetzt".

| Lage | Antwort der Stand-Routen |
|---|---|
| Turnier-PC verbunden | normal |
| weg, unter 20 s | `200` mit dem gehaltenen Stand |
| weg, ab 20 s | `503`, `Cache-Control: no-store`, `Retry-After: 2`, Rumpf `{"grund":"host_weg"}` |
| Namespace unbekannt | `503` — in den ersten 20 s nach dem Start des Relays `304`, wenn der Abruf eine Marke (`If-None-Match`) trägt |

Der Grund steht im **Rumpf**, weil die Slave-Brücke der fernen Halle eigene
Köpfe nicht durchreicht. Gesperrt gibt es weder `304` noch einen
Cache-Treffer — die Lage wird unter dem Schloss vor allem anderen
entschieden. Die **Start-Karenz** macht einen Relay-Neustart auf laufenden
Fernsehern unsichtbar, wenn der Turnier-PC binnen 20 s zurück ist (üblich:
1–4 s): Der Relay hat dann keinen Stand mehr, die Seite schon.

| Route | ohne Turnier-PC |
|---|---|
| `GET /{ns}/court/{id}/state` (auch `?zweck=stream`) | `503` nach der Karenz |
| `GET /{ns}/monitor/state?device=` | `503` nach der Karenz, wenn das Gerät ein Ziel hat; sonst `200` Kopplungs-Seite mit `hostOffline: true` |
| `GET /{ns}/health` (voll und `?court=`) | `503` nach der Karenz |
| `GET /{ns}/info/preparation/state` | `503` nach der Karenz |
| `GET /{ns}/meetingpoint/state` | `503` nach der Karenz (vorher `404`) |
| `GET /{ns}/info/announce/state` | **`503` sofort** — ein `200` ohne Sprachdienst-Schlüssel läse der Ansage-Slave als „abgeschaltet" |
| `/courts`, `/info/ad/state`, Bilder, Logo, HTML-Seiten, Gerätelisten | unverändert |

**Der Namespace überlebt den Abgang des Turnier-PCs** für die Karenz, auch
ohne Tablets (sonst gäbe es im LAN+Cloud-Betrieb nichts weiterzuzeigen).
Entfernt wird er vom Aufräum-Lauf (`fegen`, alle 5 s). Ein Namespace, den
nur ein Tablet angelegt hat und der nie einen Turnier-PC sah, verschwindet
weiter sofort mit dem Tablet.

Seit v0.9.389 bleibt ein Namespace, der je einen Turnier-PC hatte, die ganze
Daten-Frist stehen (nächster Abschnitt): Zugewiesene Geräte sehen so lange
`503` und behalten ihre Zuweisung.

Die Seiten fragen bei `503` im selben schnellen Takt weiter wie bei jedem
Fehler (Monitor und Tafel alle 250 ms); `Retry-After` beachten sie nicht. Derselbe Lauf stößt am
Karenz-Ende einmal alle Monitor-Verbindungen an, damit die Meldung sofort
erscheint; kehrt der Turnier-PC danach zurück, werden sie noch einmal
angestoßen. Beim Abgang schließt der Relay die Verbindungen der
Turnierleitungs-Seiten (sie fallen auf ihren Abruf und sehen `503`).

**Close-Grund für den Turnier-PC:** Ein zweiter Turnier-PC am selben
Namespace wird mit Code `4409` und dem Text `host_belegt` geschlossen, am
Namespace-Limit mit `4429` und `voll`.

**Meldung im Dashboard (App ab v0.9.390):** Hält die Abweisung mit `4409`
länger als 30 s an, zeigt die App „Ein anderer PC ist mit diesem Turnier
verbunden" (`ws_verbindung::CloudLage`, über `TabletInfo.cloud_hinweis` in
`AlertBanner.tsx`). Die 30 s decken den Fall, dass nach einem Netzwechsel
die EIGENE tote Verbindung noch bis zu 15 s den Platz hält. Die Meldung
verschwindet, sobald eine Verbindung 5 s steht, und beim Stoppen der
Übertragung. Ein einzelner Close ohne Grund oder ein gescheiterter Aufbau
zwischen zwei Abweisungen setzt die 30 s nicht zurück (der Close-Frame kann
verloren gehen). Kommt der Close-Grund nicht bis zur App durch (älterer Relay,
ein Proxy, der Close-Codes verschluckt), erscheint sie nie — die Verbindung
verhält sich dann wie bisher.

**Werbe-Upload:** Auf `409` (Turnier-PC am Relay noch nicht eingetragen) und
`503` (zwei Uploads in Arbeit) wiederholt die App den Upload bis zu dreimal
im Abstand von 3 s; erst danach greift die gewöhnliche Pause (30 s und
länger).

`GET /health` (prozessweit) nennt zusätzlich `hosts` — die Zahl verbundener
Turnier-PCs. Das ist der Prüfpunkt vor jedem Relay-Deploy.

## Lebensdauer der Turnierdaten (seit v0.9.389)

Spec [relay-haerten](features/relay-haerten.md), PR 8 ·
ADR [0089](adr/0089-lebensdauer-des-namespace-zustands-ohne-host.md).

Bis v0.9.388 hing die Lebensdauer aller Daten am Namespace — und der blieb,
solange irgendein Tablet verbunden war. Ein vergessenes Tablet hielt Namen,
Bilder und Listen eines beendeten Turniers beliebig lange im Speicher.

| Zeitpunkt | Was geht |
|---|---|
| **sofort** beim Abgang des Turnier-PCs | Zugang zum Sprachdienst (`azure_tts`); TL-Zugänge, TL-Zustand, wartende Abrufe; TL-Verbindungen geschlossen |
| **nach 20 s** | Anzeigen gesperrt (`503`) |
| **nach 60 min** | Briefe (Namen) der Felder ohne Tablet, Aufruf-Liste, Meetingpoint, Feldliste, Feldnamen, Hallen, Freitext, Werbebilder, Logo, Monitor-Konfiguration, Geräte-Zuweisungen, Geräte- und Slave-Präsenz; für Felder ohne Tablet auch Sätze und Zustand |
| **solange ein Tablet am Feld hängt** | seine Verbindung, sein gespiegelter Zustand (enthält die Namen seines Spiels), seine Sätze und ein **Brief ohne Personen** (nur Spielkennung und „beendet") |
| **danach** | der Namespace verschwindet, sobald kein Tablet und kein offenes Ergebnis mehr hängt |

- **Der Brief ohne Personen** ist nötig, damit das Tablet weiterzählen kann
  (ohne Eintrag verwürfe der Relay jeden Punkt als Nachzügler) und damit bei
  der Rückkehr des Turnier-PCs „gleiches Spiel" gilt — sonst stünde das Feld
  auf 0:0.
- **Rückkehr vor Ablauf:** nichts ist geleert, es ist der gewöhnliche
  Reconnect.
- **Rückkehr danach:** wie nach einem Relay-Neustart — der Turnier-PC liefert
  Feldliste, Zuweisungen und Bilder neu.
- **Namespace-Limit (2000):** Ist die Tabelle voll, weicht einem neuen
  Turnier-PC der am längsten hostlose Namespace ohne Tablet. Gehaltene Daten
  können so nie ein neues Turnier aussperren; gibt es keinen solchen, wird
  der neue Turnier-PC mit `4429 voll` abgewiesen.
- Ein Namespace, den nur ein Tablet angelegt hat (nie ein Turnier-PC),
  verschwindet weiter sofort mit dem Tablet.
- **Bilder weichen früher, wenn das Bilder-Budget knapp wird:** Vor einem
  `507` gibt der Relay die Bilder von Turnieren ohne Turnier-PC frei (am
  längsten verwaist zuerst). Ihr Turnier-PC lädt beim Wiederverbinden neu
  hoch.
- Die Verdrängung am Namespace-Limit kann auch ein echtes Turnier treffen,
  dessen Turnier-PC gerade getrennt ist und das kein Cloud-Tablet hat — es
  beginnt beim Wiederverbinden wie nach einem Relay-Neustart. Vorher wäre in
  derselben Lage der neue Turnier-PC abgewiesen worden.

Im Relay-Log: `Namespace '…': seit 60 min ohne Turnier-PC — Turnierdaten
geleert`.

## Begrenzte Kanäle (seit v0.9.386)

Spec [relay-haerten](features/relay-haerten.md), PR 5 ·
ADR [0090](adr/0090-ueberlast-am-relay-und-mindestschutz.md).

Jede Verbindung hat im Relay einen Kanal für das, was an sie hinausgeht. Bis
v0.9.385 war er unbegrenzt: Nahm der Empfänger nicht ab, wuchs er im Speicher
des Relays — der alle Turniere trägt.

| Kanal zu | Größe (Nachrichten) |
|---|---|
| Turnier-PC | 1024 |
| Tablet, Substrom des Trägers | 64 |
| Monitor, Turnierleitungs-Seite | 32 |
| Träger (Sammelkanal aller Substrome) | 1024 |

**Voller Kanal = der Empfänger wird getrennt**, sobald seine
Verbindungsschleife wieder läuft. Hängt sie gerade beim Schreiben in den
Socket fest (Gerät nimmt gar nichts mehr ab), bleibt der Speicher trotzdem
begrenzt; getrennt wird dann erst, wenn das Schreiben zurückkehrt oder der
Stale-Wächter greift (Tablet, Turnier-PC, Träger: 15 s). Eine Schreibfrist am
Relay gibt es nicht. Das Senden wartet nie
(`Tx::send` nutzt `try_send`); ist der Kanal voll, setzt es ein Merkmal, und
die Verbindungsschleife **des Empfängers** beendet sich über ihren normalen
Aufräumweg (Feld freigeben, Abo austragen). Nichts wird still verworfen,
während die Verbindung weiterläuft. Das getrennte Gerät verbindet neu und
bekommt den frischen Stand: Ein Tablet meldet sich wieder an, der Relay
schickt dem Host `tablet_connected`, der Host schiebt die Zuweisung nach —
ein `MatchAssigned` geht so nicht verloren.

**Turnier-PC — Reserve gegen gezieltes Trennen.** Eine Halle mit 20 Feldern
erzeugt im üblichen Betrieb 3–4 Nachrichten je Sekunde an den Turnier-PC;
1024 reichen dafür mehrere Minuten (zählten alle Felder gleichzeitig im
schnellsten Takt, rund 9 s). Ein Turnier-PC, der nichts mehr sendet, wird
ohnehin nach 15 s getrennt.

Alles, was ein **Tablet** in diesen Kanal legt (Punkt, Ballwechsel, Abgleiche,
Zustand, Akku, Ruf, An- und Abmeldung), ist **nachrangig**
(`Tx::send_nachrangig`): Es darf das letzte Viertel nicht belegen und löst
nie eine Trennung aus — über der Marke wird es verworfen und einmal gemeldet
(`Kanal knapp – …`). Grund: Wer die Turnier-Kennung hat, kann beliebig viele
Tablet-Frames erzeugen. Könnten sie den Kanal füllen, ließe sich der
Turnier-PC gezielt trennen und sein Platz besetzen (`host-ws` hat keine
Anmeldung). Die Reserve gehört dem, was kein Tablet auslösen kann: Ergebnisse
(höchstens 16 gleichzeitig offen), Befehle der Turnierleitung, Abrufe.

Verworfene Tablet-Frames heilen: Punkt und Zustand durch den nächsten Frame,
der Punktverlauf durch den nächsten kompletten Abgleich (spätestens am
Spielende), die Anmeldung eines Tablets, sobald der Turnier-PC neu verbindet
(der Relay nennt ihm dann alle verbundenen Tablets).

`POST /{ns}/result` nimmt höchstens 16 KiB und zehn Sätze an (vorher 2 MiB
je Ergebnis).

**Bleibt offen:** Ein Monitor kann getrennt werden, wenn mehr als 32 Anstöße
auflaufen, bevor seine Schleife sie abholt (er verbindet neu und holt den
Stand). `identify`/`take_over` sind nicht getaktet. Es gibt kein Limit für
die Zahl der Verbindungen (PR 6).

**Im Relay-Log:** `Kanal voll – Tablet (Namespace '…') wird getrennt` (bzw.
Monitor, Turnierleitungs-Seite, Turnier-PC, Träger, Substrom). Im
Normalbetrieb darf die Zeile nicht auftreten; tritt sie (oder `Kanal knapp`)
beim Turnier-PC auf, hängt dessen Leitung oder sein Rechner.

## Bilder-Tor und Bilder-Budget (seit v0.9.387)

Spec [relay-haerten](features/relay-haerten.md), PR 6 ·
ADR [0090](adr/0090-ueberlast-am-relay-und-mindestschutz.md).

Werbebilder und Logo sind das Größte, was der Relay je Turnier hält (bis
12 MiB + 2 MiB). Wer nur eine Turnier-Kennung erfindet, soll sie nicht
ablegen können.

| Lage bei `POST /{ns}/monitor` | Antwort | Wirkung |
|---|---|---|
| Kein Turnier-PC verbunden | `409` | der Rumpf wird **nicht gelesen** |
| Schon zwei Uploads in Arbeit | `503` | der Rumpf wird nicht gelesen; die App versucht es erneut |
| Turnier-PC verbunden | `200` | alles gilt |
| Gesamt-Budget erschöpft | `507` | Konfiguration gilt, die neuen Bilder werden verworfen, **die bisherigen Bilder dieses Turniers bleiben**; Log-Zeile |

- **Gesamt-Budget:** 256 MiB über alle Turniere. Der Zähler sinkt von selbst,
  sobald ein Bild verschwindet (Ersetzen, Entfernen, Aufräumen des Turniers).
  Gebucht wird je Upload ganz oder gar nicht; beim Ersetzen zählen die alten
  Bilder des Turniers nicht mit.
- **Höchstens zwei Uploads gleichzeitig:** Jeder hält vorübergehend rund
  50 MiB. Die App wiederholt einen abgelehnten Upload von selbst (erste
  Wiederholung nach etwa einer Minute, danach mit wachsender Pause).
- **Texte der Konfiguration** (`layout`, Plätze vor/unter dem Namen, Farben
  im Bild-Stil) sind auf 32 Zeichen gekürzt.
- `POST /{ns}/monitor/control` antwortet ohne verbundenen Turnier-PC ebenfalls
  `409`.

**Was das Tor leistet — und was nicht.** Es hält Uploads ab, hinter denen gar
kein Turnier-PC steht. Wer `host-ws` für eine erfundene Kennung öffnet (ohne
Anmeldung möglich), kommt hindurch; rund 19 solcher Schein-Turniere füllen das
Budget. Dann bekommen **neue** Turniere keine Bilder mehr (`507`) — laufende
behalten ihre. Vorher traf derselbe Angriff den Speicher des ganzen Servers.
Bilder eines Turniers ohne Turnier-PC verfallen erst mit dem Daten-Verfall
(PR 8 der Spec).

**Nicht gebaut:** die in der Spec vorgesehene zweite Stufe „Bilder erst nach
der ersten Feldliste". Die App schickt ihre Feldliste nur, wenn BTP Felder
liefert — ohne geladenes Turnier hätte die Cloud nie Werbung gezeigt.

### Speicher-Obergrenze des Dienstes

`ops/bts-relay.service` trägt `MemoryHigh=512M`, `MemoryMax=768M`,
`MemorySwapMax=0`. Erreicht der Relay die Grenze, beendet ihn der Kernel und
systemd startet ihn nach 3 s neu — das wirkt wie ein Deploy auf alle
Turniere, schützt aber badhub.de auf demselben Server.

**Die Dienst-Datei wird nicht automatisch ausgerollt.** Handgriff, einmalig
und in einem Fenster ohne Turnier (der Neustart trennt alle Verbindungen):

```
# 1. Heutigen Verbrauch ablesen. Liegt er über 256 MiB, die Werte erst neu herleiten.
systemctl status bts-relay
# 2. Unit aus dem Repo auf den Server bringen und einsetzen.
sudo cp ops/bts-relay.service /etc/systemd/system/
# 3. Neu laden und neu starten.
sudo systemctl daemon-reload && sudo systemctl restart bts-relay
# 4. Prüfen.
systemctl show bts-relay -p MemoryMax
curl -s https://badhub.de/bts-relay/health
```

Zurück: die drei Zeilen entfernen, `daemon-reload`, Neustart.

`ops/nginx-bts-relay.conf` nennt jetzt `client_max_body_size 25m;` — das gilt
am Server bereits, dort ist nichts zu ändern.

## Sicherheit

- **Datenschutz — was der Relay wie lange hält (seit v0.9.389):** Spielernamen,
  Schiedsrichternamen, Freitext-Durchsagen und Bilder liegen nur im
  Arbeitsspeicher, nie auf der Platte. Ohne Turnier-PC sind sie nach
  spätestens 60 Minuten gelöscht, der Zugang zum Sprachdienst sofort
  (Abschnitt „Lebensdauer der Turnierdaten"). **Ausnahme:** Der gespiegelte
  Zustand eines Tablets, das noch am Feld verbunden ist, bleibt, solange es
  verbunden ist — und er enthält die Namen und Karten **seines** Spiels. Ein
  vergessenes, eingeschaltetes Tablet hält also die Daten eines Spiels, nicht
  mehr die des Turniers. Ein Relay-Neustart leert alles.
- Die `install_id`-UUID ist der Zugangs-Token – dasselbe Modell wie die
  heutige LAN-URL. Der Relay weist Namespaces ab, die nicht wie eine
  kanonische UUID aussehen.
- Genau **ein Host pro Namespace**: eine zweite Host-Verbindung wird
  serverseitig abgewiesen → kein Host-Takeover. **Ausnahme
  (Zombie-Host-Ablösung, Cluster A3):** Ist der eingetragene Host
  nachweislich stumm (≥ 15 s weder Frame noch Pong — eine tote
  TCP-Verbindung, z. B. nach einem Netzwechsel des Masters), ersetzt ihn
  die neue Verbindung. Ein lebendiger Host pongt alle 5 s
  (Host-Ping-Takt) und kann daher nie verdrängt werden; zusätzlich
  beendet sich eine ≥ 15 s stumme Host-Verbindung selbst und gibt den
  Slot frei. Frames einer verdrängten Alt-Verbindung werden verworfen
  (Sender-Guard). Turnier-Befund 19.07.2026: ohne diese Ablösung hielt
  eine TCP-Leiche den Slot 17 Minuten — der Master war ausgesperrt.
  **Bewusste Sicherheits-Abwägung:** Die `install_id` ist der
  Zugangs-Token (R6). Vorher konnte ein Angreifer mit geleakter ID den
  Slot nur bei komplett geschlossener Master-Verbindung besetzen — jetzt
  reichen 15 s Master-Stille. Das ist der Preis des Zombie-Fixes und
  akzeptiert, weil (a) ein gesunder Master alle 5 s pongt und im Betrieb
  praktisch nie 15 s stumm ist, (b) der Relay die Abweisung des echten
  Masters als „Zweiter Host … abgewiesen" protokolliert und ihm seit
  v0.9.388 den Close-Grund `4409 host_belegt` nennt — **am Bildschirm des
  Masters steht davon bis zum letzten Schritt der Spec `relay-haerten`
  nichts** (die frühere Aussage „der Master sieht die Warnung" war falsch:
  Sie stand nur im Relay-Log) — und (c) bei
  geleakter ID der Namespace ohnehin als kompromittiert gilt →
  Roadmap-Feature „Master-Identität umziehen" ist die eigentliche
  Gegenmaßnahme.
- **Tote-Tablet-Slot in ~15 s frei (Cluster D):** Die Tablet↔Relay-Strecke
  spiegelt jetzt das `host_conn`-Muster — der Relay pingt jedes Tablet alle
  **5 s** (`TABLET_PING`), und bleibt ein Lebenszeichen (Frame **oder** Pong)
  länger als **15 s** (`TABLET_STALE`, = 3 verpasste Pongs) aus, beendet sich
  die Verbindung selbst und gibt den Court-Slot frei (statt bis zum ~30-s-
  Ping-Sendefehler). Der Browser auto-pongt auf **Protokoll-Ebene** — immun
  gegen die JS-Timer-Drosselung backgroundeter mobiler Seiten, also kein
  Fehl-Drop eines lebenden Feldes; ein 5–10-s-WLAN-Hänger (< 15 s) ebenso
  wenig. Der `detach_tablet`-Slot-Guard (`same_channel`) bleibt: eine per
  Reclaim (dasselbe Gerät) abgelöste Alt-Verbindung räumt dem neuen Tablet
  beim Stale-Drop nichts weg (R4). Ein Fehl-Drop wäre harmlos (Reconnect +
  Reclaim, Stand persistiert) → kein Kill-Switch.
- **Half-open Host-Client in ~15 s erkannt (Cluster D):** Der Host-Client
  (`relay_client.rs`) verwirft eine Verbindung, auf der **15 s** kein
  Lebenszeichen (Frame oder Relay-Ping) eintrifft (`RELAY_READ_IDLE`,
  Option A) → `run` reconnectet mit frischem Socket (Backoff-Reset). So
  reconnectet ein stiller Master bei half-open TCP (Netz weg, kein RST) in
  ~15 s statt nach dem OS-TCP-Timeout (Minuten). **Kopplung:** Die Schwelle
  nutzt den bestehenden Relay-Ping und setzt `HOST_PING ≤ 5 s` voraus — ein
  bewusster, im Code + [ADR 0020](adr/0020-tote-verbindung-read-idle-tablet-stale.md)
  dokumentierter Mono-Repo-Vertrag; kein zusätzlicher Client-Ping.
- bts-light validiert jedes eingehende Ergebnis (`process_result`):
  Match-ID muss zum aktuellen Court-Match passen, Satzstand plausibel.
  Diese Prüfung ist dieselbe wie im LAN-Modus.
- **Turnierleitungs-Zugänge** sind ein **eigener** Satz Tokens, unabhängig
  von der `install_id`: Sie stehen in keiner Adresse, gelten nur solange der
  Turnier-PC sie nennt, und ein Widerruf greift mit dem nächsten Push. Der
  Relay stellt selbst keine aus und behält keinen über das Turnier hinaus —
  verschwindet der Turnier-PC, verfallen sie samt Wegweiser und
  Anzeige-Zustand. Ohne Opt-in am Turnier-PC (`tl_web.enabled`, Default aus)
  kennt der Relay **kein** Token, und jede Anfrage endet abgewiesen, bevor
  sie Zustand berührt.
- **Tablet-Zustand: Deckel und Monitor-Fassung (seit v0.9.374):** Der per
  `state_sync` gespiegelte Zustand ist auf `relay_proto::MAX_STATE_LEN`
  (64 KiB) begrenzt — dieselbe Konstante gilt jetzt am Turnier-PC. Ein
  größerer Zustand wird verworfen, der vorige bleibt, und der Relay schreibt
  **eine Warnung je Feld und Spiel** (Feld, Match, Länge — nie Inhalt; erst
  nach der Halter-Prüfung, damit ein Fremder keine Zeilen erzeugt). Dasselbe
  gilt für den Zustand im `ScoreUpdate`-Spiegel des Hosts. Vorher geschah das
  still. Das Tablet bleibt seit Format 2 von sich aus darunter
  ([tablet.md](tablet.md), „Gespiegelter Zustand"). Monitore bekommen aus dem
  Zustand nur die schlanke Fassung ohne `history`, `rallyLog` und `events`
  (`display_court_state`); ein übernehmendes Tablet weiterhin den vollen.
- **Stale-Filter (Cluster A4):** `score_update`/`state_sync` tragen die
  Match-ID des gezählten Spiels; Relay UND Host verwerfen Frames, deren
  Match nicht (mehr) zum Feld passt — ein nach Doze/Reconnect im alten
  Spiel hängendes Tablet kann den beim Match-Wechsel geleerten
  Score-Cache nicht wieder mit dem alten Stand befüllen (Turnier-Befund
  HM-03; dasselbe Prinzip wie Tilos „stale panel rejected"). Alte
  Tablet-Seiten ohne das Feld (matchId 0) laufen ungefiltert wie bisher.
- Broker-Limits gegen Überlast: maximale Anzahl Namespaces, Tablets je
  Namespace und gleichzeitig offener Ergebnis-Übermittlungen.
- **Telefon-Kopplungscode** ([ADR 0004](adr/0004-telefon-kopplungscode.md),
  v0.9.145): 8-stelliger Zahlen-Code als kurzlebiger Alias auf den
  Namespace — nur im RAM, 1 h TTL, ein aktiver Code je Namespace,
  Ausstellung nur bei verbundenem Host, globales Fehlversuchs-Limit beim
  Einlösen (429). Die dauerhafte Bearer-Capability bleibt die
  `install_id`-UUID.
- **Azure-TTS-Vererbung** ([ADR 0003](adr/0003-azure-tts-vererbung-relay.md),
  v0.9.145): Der Host schickt seine Azure-Speech-Config als optionales
  `azureTts`-Feld im `HostFrame::Courts`-Push; der Relay hält sie je
  Namespace **nur im RAM** und liefert sie im `AnnounceState`
  (`/{ns}/info/announce/state`) an Cloud-Ansage-Slaves aus. Damit liegt ein
  rotierbares Secret im Namespace — Zugriffsmodell bleibt die
  `install_id`-Bearer-UUID (bewusste Abwägung, siehe ADR). Der Key darf in
  Relay-Logs **nie** auftauchen. Alte Relays/Hosts bleiben kompatibel
  (optionales Feld, `#[serde(default)]`); ohne neuen Relay entfällt nur die
  Vererbung. Seit **v0.9.169** trägt `AzureTtsShare` zusätzlich
  `discipline_voices` (Disziplin-Kürzel → Stimme) → die ferne Halle sagt
  dieselben Disziplinen mit denselben Stimmen an wie der Master; fürs
  Slave-Frontend als `CloudAnnounce.azure_discipline_voices` exponiert. Auch
  serde-abwärtskompatibel (alter Master ohne Feld → leere Zuordnung).
  Seit **v0.9.336** (Spec `ansage-elevenlabs`) zusätzlich `dienst` und je
  Dienst `elevenlabs_key`/`elevenlabs_voice`, `openai_key`/`openai_voice`,
  `gemini_key`/`gemini_voice` (Schlüssel sind Secrets, nie loggen). Gesendet
  wird, sobald der **gewählte** Dienst am Master vollständig ist, und **nur
  mit dessen** Zugangsdaten (`azure_share_aus` in `relay_client.rs`): Die
  Share ist über `GET /{ns}/info/announce/state` mit dem Namespace lesbar, ein
  eingetragener, aber ungenutzter Schlüssel reist deshalb nicht mit. Bei einem
  Nicht-Azure-Dienst bleiben auch die Azure-Felder leer — ein **alter** Slave
  sieht dann keine Stimme und spricht still mit seiner Ersatzstimme; für die
  neuen Dienste müssen Slaves mit aktualisiert werden. Leeres `dienst` (alter
  Master) = Azure. Das Slave-Frontend sieht nur `CloudAnnounce.azure_dienst`,
  nie einen Schlüssel. Der Relay muss neu ausgerollt sein, sonst fallen die
  neuen Felder beim Durchreichen weg.

## Last-/Soak-Test des Brokers (Cluster-Hebel C, ADR 0019)

Der Broker ist der geteilte Serialisierungspunkt (globaler `namespaces`-Mutex).
Ein **In-Process-Concurrency-Harness** (`relay/src/main.rs`, `#[cfg(test)] mod
load`) treibt die Eintrittspunkte aus vielen `tokio`-Tasks unter echter
**Multi-Thread-Contention** (`worker_threads = 4`) und prüft — jeweils **nach
`JoinSet::join_all`**, nur reihenfolge-unabhängige Invarianten — Massen-Connect,
Reconnect-Sturm (genau ein Halter je Court, `T-1` Superseded), Nudge-Fan-out
(Namespace-Isolation), Ergebnis-Schwall (`pending` leer, `MAX_PENDING_PER_NS`)
und Cleanup (`is_empty`).

**Bewiesen:** Cap-Einhaltung, Ownership-End-Invariante, Namespace-Isolation,
Aufräumen, kein Panic + Terminierung. **Bewusst NICHT bewiesen** (der Harness
leert die mpsc-Empfänger selbst): Socket-`send().await`-Backpressure und
unbegrenztes Wachstum der `UnboundedSender`-Queue bei zähem Socket, HTTP-Layer,
Scheduling-Reihenfolge — „Deadlock-Freiheit" wird nicht behauptet (der Broker
hält den Mutex nie über `.await`). Diese Socket-/Netz-Realität deckt die
**manuelle 36-Geräte-Messung** im echten WLAN ab. Die leichte Variante läuft in
der CI, die Soak-Variante manuell: `cargo test -p bts-relay -- --ignored`.

## Deployment

Der Relay läuft als systemd-Dienst auf dem Hetzner-Server (`178.104.221.177`)
hinter nginx.

**Binary** – wird per GitHub-Actions gebaut und deployt
(`.github/workflows/relay-deploy.yml`, Trigger: Änderungen an `relay/`,
`relay-proto/` oder an einer der Seiten, die das Relay per `include_str!`
einbettet, auf `main`, plus `workflow_dispatch`). Wer im Relay eine Seite
neu einbettet, muss sie in die `paths` des Workflows aufnehmen — sonst bleibt
eine Änderung an ihr in der Cloud unsichtbar. Die CI prüft das
(`scripts/test-relay-deploy-pfade.mjs`).
Reproduzierbar gebaut, kein Rust-Toolchain auf dem Prod-Server nötig.

**Sicherung als Rückweg (seit v0.9.261).** Vor dem Umbenennen auf die neue
Binary kopiert der Workflow die laufende unter `/opt/bts-relay/bts-relay`
nach `/opt/bts-relay/bts-relay.rollback`. Der Rückweg bei einer nicht
anspringenden Version:

```sh
ssh bts-deploy@178.104.221.177 \
  'mv /opt/bts-relay/bts-relay.rollback /opt/bts-relay/bts-relay \
   && sudo systemctl restart bts-relay \
   && sleep 2 \
   && curl -sf http://127.0.0.1:8090/health'
```

Das `cp` braucht kein sudo (das Verzeichnis ist der Gruppe `bts-deploy`
schreibbar); das einzige sudo bleibt der eine Restart-Befehl.

**Zwei getrennte Benutzer (seit 2026-08-12).** Vorher lief der Dienst als `badhub`
und GitHub Actions deployte als `badhub` — ein Benutzer mit `NOPASSWD: ALL`. Damit war
der Deploy-Schlüssel faktisch ein Root-Schlüssel für den badhub-Produktivserver, und
zwar auch nach einer Trennung des Deploy-Benutzers: Wer die Relay-Binary austauschen
und den Dienst neu starten darf, bekäme Code-Ausführung als `badhub`. Deshalb sind
**beide** Rollen getrennt:

| Benutzer | Rolle |
|---|---|
| `bts-relay` | führt `bts-relay.service` aus — Systembenutzer, `nologin`, kein sudo |
| `bts-deploy` | GitHub Actions deployt als dieser Benutzer — kein sudo ausser einem Befehl |
| `badhub` | bleibt der administrative Benutzer, unverändert |

Gegengeprüft: Ersetzt `bts-deploy` die Binary und startet neu, läuft der Code als
`bts-relay` — nicht als `badhub`, nicht als root.

**Einmalige Server-Einrichtung:**

```sh
# Benutzer: Dienst und Deployment getrennt, beide ohne allgemeines sudo
sudo useradd --system --no-create-home --home-dir /nonexistent \
     --shell /usr/sbin/nologin bts-relay
sudo useradd --create-home --shell /bin/bash bts-deploy

# Verzeichnisse: Eigentuemer bleibt badhub (haelt den alten Weg als Rollback),
# Gruppe bts-deploy schreibt, setgid vererbt sie an neue Dateien.
sudo mkdir -p /opt/bts-relay
sudo chown badhub:bts-deploy /opt/bts-relay && sudo chmod 2775 /opt/bts-relay
sudo chown badhub:bts-deploy /var/www/badhub/public/download/bts-light
sudo chmod 2775 /var/www/badhub/public/download/bts-light

# Log-Verzeichnis: der Dienst schreibt ueber die Gruppe. Das setgid-Bit ist
# PFLICHT — ohne es kann er nach dem naechsten Tageswechsel nicht mehr schreiben.
sudo chgrp -R bts-relay /var/www/badhub/storage/relay-logs
sudo chmod 2775 /var/www/badhub/storage/relay-logs

# systemd-Unit installieren (User=bts-relay)
sudo cp ops/bts-relay.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now bts-relay

# sudoers: GENAU ein Befehl, keine Wildcards. Erst validieren, dann installieren —
# eine kaputte sudoers-Datei sperrt jeden sudo-Zugang aus.
echo 'bts-deploy ALL=(root) NOPASSWD: /usr/bin/systemctl restart bts-relay' \
  | sudo tee /etc/sudoers.d/bts-deploy
sudo chmod 0440 /etc/sudoers.d/bts-deploy && sudo visudo -c

# SSH-Schluessel fuer den Deploy — vier Restriktionen, kein Forced Command
# (der Deploy braucht mehrere Befehle: rsync, mv, curl, ls).
sudo -u bts-deploy ssh-keygen -t ed25519 -N "" -f /home/bts-deploy/.ssh/id_ed25519
# Oeffentlichen Teil mit Praefix in ~bts-deploy/.ssh/authorized_keys eintragen:
#   no-pty,no-port-forwarding,no-agent-forwarding,no-X11-forwarding ssh-ed25519 …
```

**Deployment-Weg.** `git push`/Tag → GitHub Actions → SSH als `bts-deploy` →
Deployment. Vollständig unattended: **kein Passwort**, keine manuelle SSH-Sitzung,
keine Freigabe-Klicks. Der private Schlüssel liegt im Repo-Secret
`SSH_DEPLOY_KEY_V2`, `SSH_KNOWN_HOSTS` ist unverändert.

> **Rollback.** Das alte Secret `SSH_DEPLOY_KEY` und der alte Schlüssel
> `github-actions-bts-light-deploy` in `/home/badhub/.ssh/authorized_keys` bleiben
> vorerst bestehen. Zurück heißt: in den beiden Workflows `bts-deploy@` → `badhub@`
> und `SSH_DEPLOY_KEY_V2` → `SSH_DEPLOY_KEY`. Deshalb wurde ein **zweites** Secret
> angelegt statt das bestehende zu überschreiben — GitHub-Secrets sind nicht
> auslesbar, ein Überschreiben wäre unumkehrbar gewesen.
>
> Bestehende Zugänge sind unverändert: `badhub` (Administration) und `tilo`
> (SFTP-Datenlieferung).

**nginx** – den `location /bts-relay/`-Block aus `ops/nginx-bts-relay.conf`
in den `badhub.de`-Server-Block (Port 443) übernehmen, plus den
`map $http_upgrade $connection_upgrade`-Block im `http{}`-Kontext. Danach
`sudo nginx -t && sudo systemctl reload nginx`.

Der Dienst lauscht auf `127.0.0.1:8090` (`PORT`), QR-Codes zeigen auf
`PUBLIC_BASE` (Default `https://badhub.de/bts-relay`).

## Fehlersuche

- `https://badhub.de/bts-relay/health` antwortet mit `{"ok":true,…}` →
  Relay läuft und ist über nginx erreichbar.
- **Relay-Log als Datei** (empfohlen, ohne journal-Recht): bei gesetzter
  `RELAY_LOG_DIR` (systemd-Unit → `storage/relay-logs/`) schreibt der Relay
  täglich rotierend nach `bts-relay.log.YYYY-MM-DD`; der `badhub`-User liest
  sie direkt per SFTP/SSH. Zeigt Verbindungen, Übernahmen und ob beim
  (Neu-)Verbinden ein Spielstand wiederhergestellt wurde (StateRestore) oder
  das Feld bei 0:0 startet. Details: [logging.md](logging.md) → „Relay-Log".
  **Nach Unit-Änderung einmalig:** `sudo systemctl daemon-reload && sudo systemctl restart bts-relay`.
- `journalctl -u bts-relay -f` zeigt dasselbe live (benötigt `systemd-journal`-Recht).
- Tablet erreicht die Seite, aber „verbinde…" bleibt → bts-light ist im
  Cloud-Modus nicht verbunden (App-Log prüfen: „Mit Cloud-Relay
  verbunden") oder ein zweiter Host belegt den Namespace.
- Ergebnis-Übermittlung meldet „Zeitüberschreitung" → bts-light hat nicht
  geantwortet; meist BTP-seitig (Netzwerk-Edits in BTP nicht erlaubt).
- **Wiederkehrende „Host unbekannt"-Fehler im App-Log** → der DNS des
  Hallen-Routers ist unzuverlässig (Turnier-Log 19.07.2026: 23 Ausfälle
  an einem Tag; der Backoff-Reconnect heilte jeden). **Empfehlung für
  den Turnier-PC:** in den Windows-Netzwerkeinstellungen einen
  öffentlichen DNS eintragen (bevorzugt `1.1.1.1`, alternativ `8.8.8.8`)
  — Adaptereinstellungen → IPv4 → „Folgende DNS-Serveradressen
  verwenden". Das macht Liveticker und Cloud-Relay unabhängig vom
  Router-DNS.

## Meetingpoint (seit v0.9.330, ADR 0074)

- `GET /{ns}/meetingpoint?halle=` — Seite; `GET /{ns}/meetingpoint/state?halle=`
  — aus dem zuletzt gepushten Gesamtstand (`HostFrame::Meetingpoint{view}`,
  alle Hallen, gedeckelt auf `MAX_MP_PARTIEN`) per
  `MeetingpointView::fuer_halle` zugeschnitten; 404, solange der Host den
  Meetingpoint nicht eingeschaltet meldet.
- `POST /{ns}/meetingpoint/checkin` (Body `MpCheckinBody`, ≤ 2 KiB) →
  `RelayFrame::MeetingpointCheckin{reqId, body}` an den Host →
  `HostFrame::MeetingpointAck{reqId, ok, error}`; Muster `/result` mit
  `pending`, `MAX_PENDING_PER_NS`, `RESULT_TIMEOUT`. Der Relay urteilt nicht —
  die Prüfung (Halle, gerufene Partie, Platz) liegt beim Host.
- Der Host pusht den Stand im 2-s-Takt nur bei Änderung (Fingerabdruck).
- Die Seite liegt unter der Namespace-Adresse wie Tafel/Anzeige; dass die
  Kennung dort ablesbar ist, ist bewusst hingenommen (Abgrenzung ADR 0012).
- `MpPartie.feldFreiMs` (Unix-ms, Uhr des Hosts) + `feldFreiUnsicher`
  (seit v0.9.345, beide `serde(default)`, ohne Wert nicht serialisiert):
  Prognose „Feld frei" für den Countdown der Seite. **Absolut** statt als
  Restdauer, weil der Relay den Stand nur bei Änderung bekommt; die Seite
  rechnet gegen den `Date`-Kopf der Antwort. Ein Relay vor v0.9.345 lässt
  die Felder beim Durchreichen fallen → kein Countdown, sonst unverändert.
- `MpPartie.aufgerufenFeld` (seit v0.9.347, `serde(default)`, ohne Wert nicht
  serialisiert): Die Partie kam gerade aufs Feld. Die Seite zeigt sie noch
  `MP_AUFGERUFEN_ANZEIGE_MS` (2 min) als „Aufgerufen – Feld N". Ein alter
  Relay lässt das Feld fallen, dann erscheint eine leere Karte ohne Hinweis.
  Der Relay deployt beim Merge aber vor jeder App.
- `MpPartie.ar` (seit v0.9.361, ADR 0080, `serde(default)`, ohne Wert nicht
  serialisiert): Name des Aufschlagrichters, **nur Anzeige** (kein
  Check-in-Platz). Der Relay deckelt ihn wie alle Texte auf `MAX_MP_TEXT`.
  Ein alter Host schickt das Feld nicht, dann zeigt die Seite keinen AR.
- `TlAction::CalledReorder { matchId, beforeMatchId? }` (JSON
  `called_reorder`, seit v0.9.347, ADR 0077): ein gerufenes Spiel in der
  Aufruf-Reihenfolge verschieben, nur innerhalb derselben Aufruf-Halle.
  Fehler kommen als `NotAllowed` mit Text zurück. Der Relay leitet die Aktion
  typisiert weiter, ohne eigene Logik. Die Seite zeigt Griffe nur bei
  `TlState.can_reorder_called`.
- **Ballzahl (seit v0.9.398, Spec
  [tablet-ballzaehler](features/tablet-ballzaehler.md), ADR 0093):**
  `ResultBody.shuttles` und `RelayFrame::Result.shuttles` (optional, 0–99)
  tragen die am Tablet gezählten Bälle zum Host. Der Relay baut den
  `Result`-Frame Feld für Feld und reicht die Zahl durch; ein Wert über
  `MAX_SHUTTLES` fällt dort schon weg (`ballzahl_geprueft`), das Ergebnis geht
  weiter. `MatchBrief.shuttleCounter` (`serde(default)`) ist Schalter und
  Fähigkeitsmeldung in einem — ohne das Feld zeigt das Tablet keinen Zähler.
  `TlAction::EnterResult.shuttles` (optional) nimmt die Zahl der
  Turnierleitung auf; fehlt es, bleibt der Wert in BTP. `shuttles` im
  gespiegelten Tablet-Stand streicht `schlanker_anzeige_stand` aus allen
  Anzeige-Zuständen. Im TL-Zustand (opak für den Relay) reisen seit v0.9.400
  `shuttle_counter`, `TlCourt.shuttles` und `TlFinished.shuttles`. **Deploy-Reihenfolge: Relay vor App** — ein alter Relay
  wirft die Zahl still weg und lehnt eine `EnterResult`-Aktion mit dem
  unbekannten Feld nicht ab, reicht sie aber ohne Zahl weiter.
- Neu: `TlAction::MeetingpointSet`, `MatchBrief.feldCheck` (`serde(default)`).
  Neue Enums `MpPlatz`/`MpOrt` haben einen `serde(other)`-Auffang.
  **Deploy-Reihenfolge: Relay vor App** (der Relay parst `TlAction`; ein alter
  Relay kennt die Routen nicht).
