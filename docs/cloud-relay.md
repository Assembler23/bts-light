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
5. „Ergebnis übermitteln" → `POST …/result` → Relay reicht es per
   WebSocket-Frame an den Host → bts-light schreibt per `SENDUPDATE` nach
   BTP und antwortet mit `ResultAck`. Der Relay wartet auf die `ResultAck` nur
   **8 s** (`RESULT_TIMEOUT`, seit Hebel B / ADR 0018; vorher 20 s) — danach
   `ok:false`, der Client puffert und retryt ohnehin (idempotent). Das kürzere
   Warten gibt den `pending`-Slot (`MAX_PENDING_PER_NS`) schneller frei. Details:
   [tablet.md](tablet.md) („Ergebnis-Übermittlung verlustsicher").

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

**Hallen-Vorverteilung** (Spec `hallen-vorverteilung`): zwei neue
`TlAction`-Varianten `set_hall_prefill { enabled, window }` und
`clear_auto_halls` — der Relay parst Aktionen **typisiert**, ein alter
Relay lehnt sie ab → auch hier Relay-Deploy vor dem Client-Release. Das
zusätzliche `TlState`-Feld `hall_prefill` und der neue
`hall_source`-Wire-Wert `"auto"` sind additiv; alte Seiten tolerieren
beide (Feature-Detection über das State-Feld — eine neue Seite an einem
alten Host zeigt die Bedienelemente gar nicht).

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

### Erneute Aufrufe — je Partei

Beide Aufruf-Aktionen können auf **eine Partei** eingegrenzt werden; die
Partei reist als `relay_proto::PrepCallSide` (`"both"` / `"team1"` /
`"team2"`).

| `TlAction` | Partei-Feld | Verhalten |
|---|---|---|
| `announce_prep_call` | `side` — **Pflicht** | Nachruf am Meeting Point. Die Stufe wird **je Partei** gezählt (`prep_call_stages`); die eine kann längst da sein, während die andere fehlt. |
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
  `TlDisplaySettingsWire` zusätzlich `showCourtRemaining`
  (`#[serde(default)]` — alte Browser-Profile ohne das Feld lesen sich als
  „aus"); das zugehörige Anzeigedatum reist als `TlCourt.remaining_min`
  im opaken `TlState`-JSON mit (Serde-Default, alte Gegenstellen
  ignorieren es). Seit 17.08.2026 trägt `TlDisplaySettingsWire` außerdem
  `unlimitedCourtCalls` (gleiche `#[serde(default)]`-Abwägung: fehlt das
  Feld, bleibt der bisherige Deckel bei drei Aufrufen) — die Wirkung ist
  rein clientseitig, der Turnier-PC zählt Aufruf-Stufen seither lediglich
  ehrlich über 3 hinaus weiter.
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
`relay-proto/` oder `tablet.html` auf `main`, plus `workflow_dispatch`).
Reproduzierbar gebaut, kein Rust-Toolchain auf dem Prod-Server nötig.

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
