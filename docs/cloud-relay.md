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
| `GET /{ns}/qr/{label}` | QR-Code (SVG) auf die öffentliche Court-URL |
| `GET /{ns}/ws` | Tablet-WebSocket |
| `GET /{ns}/host-ws` | bts-light-Host-WebSocket (ausgehend) |
| `POST /{ns}/result` | Endergebnis vom Tablet → an den Host weitergereicht |
| `POST /{ns}/pairing-code` | Telefon-Kopplungscode ausstellen (ADR 0004, nur bei verbundenem Host) |
| `GET /pair/{code}` | Telefon-Code → Namespace auflösen (1 h TTL, Fehlversuchs-Limit) |
| `GET /tl`, `GET /tl/api/state`, `POST /tl/api/command` | Turnierleitungs-Oberfläche — **ohne Namespace in der Adresse**, siehe unten |
| `GET /health` | Status-Schnappschuss |

### Datenfluss

1. bts-light verbindet sich im Cloud-Modus ausgehend zu
   `wss://badhub.de/bts-relay/<install_id>/host-ws`.
2. Ein Tablet öffnet `…/court/<court>` und verbindet seine WebSocket. Der
   Relay meldet dem Host `tablet_connected`.
3. Der Host pusht alle 2 s die Court→Match-Zuweisung; der Relay leitet sie
   an das jeweilige Tablet.
4. Jeder Punkt am Tablet → `score_update` → Relay → Host → Liveticker.
5. „Ergebnis übermitteln" → `POST …/result` → Relay reicht es per
   WebSocket-Frame an den Host → bts-light schreibt per `SENDUPDATE` nach
   BTP und antwortet mit `ResultAck`.

**Aufgerufene Spiele in die ferne Halle (Cluster C Stufe 2, v0.9.154):**
Der Host pusht seine in Vorbereitung gerufenen Spiele als
`HostFrame::Prepared` (nur bei Änderung, Fingerabdruck) an den Relay; der
hält sie je Namespace (`Namespace.prepared`) und gibt sie in
`GET /{ns}/info/announce/state` **hallengefiltert** als
`AnnounceState.prepared` zurück. Der Cloud-Slave zeigt sie unter
„Aufgerufene Spiele" und sagt den **Zweit-/Drittaufruf** einer fehlenden
Partei **lokal** in seiner Halle an (kein Rückkanal zum Master). Details:
[announcements.md](announcements.md).

### Turnierleitungs-Geräte (TL-Web) — Wire-Ebene

> **Stand:** Im **LAN** ist die Oberfläche vollständig bedienbar (siehe
> [tablet.md](tablet.md)). Der **Relay** hat seit Schritt 10 Routen,
> Zugangsprüfung und Weiterleitung — was noch fehlt, ist die Gegenseite am
> Turnier-PC (`relay_client.rs` pusht `TlAuth`/`TlState` noch nicht und
> beantwortet `TlCommand` noch nicht). Bis dahin bleibt der Cloud-Weg
> unbenutzt: Ohne Push kennt der Relay kein einziges Token, und **jede**
> Anfrage endet abgewiesen, bevor sie irgendetwas berührt. Fachliche
> Grundlage: [features/turnierleitung-web.md](features/turnierleitung-web.md),
> [ADR 0010](adr/0010-tl-web-schreibender-cloud-pfad.md) und
> [ADR 0011](adr/0011-tl-web-geraete-identitaet.md).

Turnierleitungs-Geräte sind eine **dritte Client-Klasse** neben Tablets und
Monitoren: mehrere je Namespace (höchstens 8), **nicht** feldgebunden, und
sie schreiben ausschließlich **über den Host**. Sie landen nie in der
Tablet-Liste eines Namespace und übernehmen nie eine Court-Session — R4
(„ein aktives Tablet je Court") bleibt unberührt.

Die geteilten Typen in `relay-proto`:

| Typ | Zweck |
|---|---|
| `TlAction` | Der **geschlossene** Satz erlaubter Aktionen (Feld belegen/räumen/umhängen, Vorbereitungs-Aufruf, erneuter Aufruf, Ergebnis, Walkover, Zähltafelbediener, Auto-Vergabe). Was hier nicht steht, ist nicht darstellbar. |
| `CourtExpectation` | Was das Gerät auf dem Feld **vorgefunden** hat (`any` / `free` / `match`). Stimmt es nicht mehr, lehnt der Host ab — so überschreiben zwei Geräte einander nicht stillschweigend. |
| `TlResponse` + `TlErrorCode` | Antwort mit **maschinenlesbarem** Grund, damit die Seite gezielt reagieren kann, plus der Revision, auf die sie sich neu ausrichten soll. Kennt auch „ausgeführt, aber mit Hinweis" (etwa: in dieser Halle ist kein Ansage-Gerät verbunden) — ausdrücklich kein Fehler. |
| `RelayFrame::TlCommand` | Kommando an den Host; `reqId` korreliert die Antwort, `opId` ist der Idempotenzschlüssel gegen doppelte Schreibvorgänge nach einem Netzwackler, `viewRev` die Revision der Ansicht, auf der die Aktion beruhte (Grundlage der Altersprüfung). |
| `HostFrame::TlAck` | Die Quittung — der Absender erfährt das Ergebnis **nach** dem BTP-Schreiben, kein Fire-and-forget. |
| `HostFrame::TlAuth` | Die zugelassenen Geräte als Paare aus **Kennung und Zugang** — ohne Namen (Datensparsamkeit). Der Host stellt sie aus, der Relay spiegelt sie nur; die Liste **ersetzt** die bisherige, und genau das ist der Widerruf. Die Kennung reist mit jedem Kommando zurück, damit das Protokoll des Turnier-PCs benennen kann, wer gehandelt hat, ohne dass ein Zugang in Protokollen auftaucht. |
| `HostFrame::TlState` | Der Anzeige-Zustand als **opakes** JSON plus Revision. Der Relay legt ihn nur ab und liefert ihn unverändert aus — wie beim Court-Zustand bleibt die Turnierlogik vollständig im Host. |

**Die `install_id` verlässt den Master dabei nicht.** Anders als bei
Tablets und Monitoren ist der Namespace kein Bestandteil der
TL-Adressen; der Relay schlägt ihn über das Gerätetoken nach (ADR 0011).
Der Grund ist handfest: Die `install_id` **ist** der Zugang der Zähltablets
(`/{ns}/ws`). Stünde sie in der Adresse, die jeder Helfer den ganzen Tag auf
dem Bildschirm hat, könnte sich damit jeder als Tablet ausgeben.

**Routen im Relay** (seit Schritt 10, alle ohne Namespace):

| Route | Zweck |
|---|---|
| `GET /tl` | Die Seite selbst — **ohne** Zugangsprüfung, wie die Tablet-Seite. Ausgeliefert wird eine leere Hülle, die ihren Zugang erst aus dem Adress-Fragment (`#t=…`) liest; alles Verwertbare kommt über die geschützten API-Routen. |
| `GET /tl/api/state` | Der zuletzt gepushte Anzeige-Zustand, **unverändert** durchgereicht. Mit `ETag` aus **Host-Generation und Revision**: Ein Gerät, das denselben Stand schon hat, bekommt `304` — bei einer Seite, die alle zwei Sekunden fragt, ist das über Mobilfunk der Unterschied zwischen sparsam und lästig. Die Generation muss hinein, weil die Revision beim Neustart des Turnier-PCs wieder klein beginnt; ohne sie bekäme ein Gerät „unverändert" auf einen völlig anderen Turnierstand. Fehlt der Stand, ist die Antwort `503` und **nicht** ein leeres Turnier: Leer sähe aus wie „alle Felder frei". |
| `POST /tl/api/command` | Kommando an den Turnier-PC, Antwort synchron über `reqId`/`TlAck` — dasselbe erprobte Muster wie die Ergebnismeldung vom Tablet, mit 20 s Zeitablauf. |

Der Relay ist dabei **Briefträger, nicht Schiedsrichter**: Er kennt weder
Spiele noch Felder, prüft den Zugang und reicht durch. Ob eine Aktion
zulässig ist, entscheidet allein der Turnier-PC (R5) — er ist der Einzige mit
dem Turnierstand. Ein Fehler im Relay kann deshalb keine Wertung erfinden.

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

**Not-Aus:** `BTS_RELAY_TL=off` lässt die drei Routen gar nicht erst
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
schickt der Relay wie bisher als `state_restore` — das Tablet wendet ihn
aber nur noch an, wenn er **neuer** ist als sein lokaler Stand
(Stand-Revision `rev` im Snapshot, „neuer gewinnt"), sonst repariert es
den Relay-Cache mit den offline weitergezählten Punkten. Details:
[tablet.md](tablet.md).

## Sicherheit

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
  praktisch nie 15 s stumm ist, (b) der echte Master beim Reconnect die
  „Zweiter Host"-Warnung sieht (Übernahme fällt auf) und (c) bei
  geleakter ID der Namespace ohnehin als kompromittiert gilt →
  Roadmap-Feature „Master-Identität umziehen" ist die eigentliche
  Gegenmaßnahme.
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

## Deployment

Der Relay läuft als systemd-Dienst auf dem Hetzner-Server (`178.104.221.177`)
hinter nginx.

**Binary** – wird per GitHub-Actions gebaut und deployt
(`.github/workflows/relay-deploy.yml`, Trigger: Änderungen an `relay/`,
`relay-proto/` oder `tablet.html` auf `main`, plus `workflow_dispatch`).
Reproduzierbar gebaut, kein Rust-Toolchain auf dem Prod-Server nötig.

**Einmalige Server-Einrichtung:**

```sh
# Verzeichnis anlegen, dem Deploy-User schreibbar
sudo mkdir -p /opt/bts-relay && sudo chown badhub:badhub /opt/bts-relay

# systemd-Unit installieren
sudo cp ops/bts-relay.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now bts-relay

# sudoers: badhub darf den Dienst neu starten (für den CI-Deploy)
echo 'badhub ALL=(root) NOPASSWD: /usr/bin/systemctl restart bts-relay' \
  | sudo tee /etc/sudoers.d/bts-relay
```

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
