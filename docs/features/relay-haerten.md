# Relay härten — Spezifikation

> Status: **abgestimmt 2026-11-11** (freigegeben vom Nutzer; via /idee: Brief → Grill → How-To → Review).
> Quelle: Kommunikations-Audit vom 10.11.2026 (Paket 3, siehe [roadmap.md](../roadmap.md)).
> Betroffene Crates: relay · relay-proto · src-tauri (Rust-Kern, eingebettete Seiten) · src (eine Dashboard-Meldung).
> ADR: [0088](../adr/0088-anzeigen-bei-host-verlust-karenz-dann-503.md) ·
> [0089](../adr/0089-lebensdauer-des-namespace-zustands-ohne-host.md) ·
> [0090](../adr/0090-ueberlast-am-relay-und-mindestschutz.md) ·
> [0091](../adr/0091-ergebnisse-im-host-ein-arbeiter-je-feld.md).
> Geprüft gegen main `fdfd701` (v0.9.378). Reine Code-Lektüre — nichts gebaut, nichts
> ausgeführt, nichts gemessen. Größen- und Zeitangaben sind gerechnet.

## Kontext / Problem

Der Cloud-Relay (`bts-relay` auf badhub.de) verbindet den Turnier-PC („Host") mit
Cloud-Tablets, Cloud-Anzeigen, der Turnierleitungs-Oberfläche und der fernen Halle. Er hält
allen Zustand im RAM, wird bei jedem passenden main-Merge automatisch neu ausgerollt und
steht ohne Anmeldung im Internet. Das Audit vom 10.11.2026 nannte sechs Befunde; Grill und
How-To haben sie am Code nachgeprüft, teils korrigiert und um weitere ergänzt. Maßgeblich
ist die Fassung hier.

### Belegt im Turnier

Keiner der sechs Befunde ist als Turnier-Vorfall belegt. Belegt sind nur die Verwandten:

- **19.07.2026:** Eine tote Host-Verbindung hielt den Host-Slot 17 Minuten, der Turnier-PC
  war ausgesperrt (`docs/cloud-relay.md:1128–1130`). Seitdem löst der Relay einen 15 s
  stummen Host ab — genau diese 15 s machen einen Host, der inline wartet (Befund 3),
  abwerfbar.
- **13.08.2026:** Cloud-Monitore standen auf 0:0, weil der Relay nur Stände von
  Cloud-Tablets kannte (behoben mit dem Host-Spiegel).
- **14.06.2026:** Ein Ersatz-Tablet begann bei 0:0 (behoben mit der Spec
  `tablet-stand-robust`). Der gesicherte Stand am Relay ist seither das, was ein Ersatzgerät
  bekommt — er darf durch dieses Paket nicht verloren gehen.
- Das größte Turnier hatte 26 Felder.

Als Beleg vor der Umsetzung wünschenswert, aber von hier nicht beschaffbar: die Zahl der
Zeilen „Host … stumm – Verbindung als tot verworfen", „Namespace-Limit" und „Zweiter Host"
im Relay-Log sowie der `namespaces`-Wert aus `/health` nach mehreren Wochen.

### Aus Code-Lektüre — die sechs Befunde, nachgeprüft

1. **Tote Feld-Slots.** `TabletSitzung::verarbeite` überschreibt bei jedem `identify` nur
   `self.court` (`relay/src/main.rs:2779–2798`); `abraeumen` löst nur dieses eine Feld
   (`:2889–2894`). Ein zweites `identify` mit anderer CourtID hinterlässt einen Eintrag mit
   totem Sender in `tablets`; `is_empty` (`:690–695`) ist dann nie wahr, der Namespace
   bleibt bis zum Relay-Neustart. **Korrektur am Audit:** „Der Namespace verfällt nie" ist
   zu pauschal — `namespace_aufraeumen` (`:3341–3358`) räumt ereignisgetrieben; es fehlt nur
   ein Zeit-Verfall. **Zusätzlich:** Ein Tablet kann ohne Host Einträge für beliebige
   CourtIDs erzeugen (`match_id == 0` passiert den Filter, `:3982–3990`); `court_scores`,
   `court_state` (2 × bis 64 KiB je Feld) und `monitor_seq` werden bei `detach_tablet`
   nicht geräumt (`:3895–3902`). `take_over_court` legt einen Namespace ohne Limit-Prüfung
   an (`:3761`). `forward_battery` ist der einzige Weiterleiter ohne Halter-Prüfung
   (`:4059–4072`).
2. **Keine wirksamen Größengrenzen.** Alle fünf WebSocket-Upgrades sind ohne Größenangabe
   (`tablet_ws :2726`, `carrier_ws :2996`, `monitor_ws :3266`, `tl_ws_route :3464`,
   `host_ws :4271`); es gilt der Bibliotheks-Standard von 64 MiB je Nachricht. Ungedeckelt
   sind `sets_history` im Tablet-Score (`:4024–4037`, gespeichert und weitergereicht),
   `device_id`, `MatchAssigned`/`MatchCleared` (Zahl der CourtIDs, Brief, Label, Halle,
   `host_sets`), Zeichenketten in `Courts`/`Prepared`, `azure_tts`, `MonitorControl`
   (`:1346`), Turniername und Konfiguration im Upload. Einen Mengen-Takt gibt es nur für die
   Weitergabe von `state_sync` (`:3939–3958`). Alle Kanäle sind `unbounded`. **Korrektur am
   Brief:** Der fünfte Weg `carrier-ws` (ferne Halle) fehlte; die größte Host-Nachricht ist
   der Schiedsrichterzettel (fachlicher Deckel 5184 KiB Roh-HTML, `:177`), nicht ein
   Werbebild (das geht per HTTP).
3. **Die Host-Schleife wartet inline.** `process_result` (`relay_client.rs:558`),
   `maybe_upload_monitor` (`:245`) und `sync_monitor_control` (`:254`) werden in der
   Leseschleife abgewartet. BTP braucht bis 5 s Connect + 10 s Read, das Schreib-Gate des
   TP-Uploads bis 20 s, der Werbe-Upload bis 15 s je Versuch und scheitert über eine
   schwache Leitung alle 30 s neu. Während des Wartens beantwortet der Host kein Ping; der
   Relay verwirft ihn nach 15 s Stille (`main.rs:4402–4407`). Das trifft alle Cloud-Felder
   und öffnet das Fenster, in dem der Host-Slot übernommen werden kann.
4. **Verbindungsaufbau ohne Frist, Backoff ohne Jitter.** `connect_async` ohne Timeout
   (`relay_client.rs:116–118`), Rücksetzen des Backoff direkt nach dem Handshake (`:119`),
   Verdopplung bis 30 s ohne Jitter (`:102–103`). Eine Abweisung sieht für den Host wie ein
   normaler Close aus (`main.rs:4326–4329`) — Dauer-Reconnect im Sekundentakt, und
   „abgewiesen" ist nicht von „Relay startet neu" zu unterscheiden. Derselbe Fehler steckt
   im Träger der fernen Halle (`carrier.rs:228–230`).
5. **Anzeigen ohne Host.** `monitor_state` (`:1200`), `monitor_device_state` (`:1232`),
   `overview_health` (`:2041`), `meetingpoint_state` (`:2393`), `preparation_state`
   (`:2527`) und `announce_state` (`:1397`) prüfen den Host nicht. **Zweite Lücke:** Fehlt
   der Namespace ganz, kommt 200 mit leerem Stand (`:1216`, `:2217–2222`) — das Feld
   erscheint frei, die Übersicht leer. Der Kommentar an `is_empty` (`:683–689`) verspricht
   das Gegenteil. **Korrektur am Vorschlag:** Ein sofortiges 503 ließe bei jedem
   Sekunden-Abriss des Turnier-PCs alle Hallen-TVs die Störung zeigen.
6. **Daten nach Host-Abgang.** Solange ein Tablet oder ein TL-Tab den Namespace hält,
   bleiben Namen und Vereine (`court_matches`, `prepared`, `meetingpoint`), Sätze,
   gesicherter Tablet-Stand, Feldliste, Freitext, Werbebilder, Logo und der
   Sprachdienst-Schlüssel (`azure_tts`, abrufbar über `:1456`) im RAM und abrufbar. Schon
   heute gelöscht werden TL-Zugänge, TL-Zustand und offene Abrufe (`:4416–4436`); `tl_subs`
   werden nicht geschlossen — ein TL-Tab mit totem Zugang hält den Namespace.

### Aus Code-Lektüre — neue Befunde des How-To

**Jeder dieser Punkte ist aus dem Code abgeleitet und nicht nachgestellt. Er wird zuerst
per rotem Test nachgewiesen, bevor er behoben wird;** bleibt der Test grün, entfällt der
Punkt und die Spec wird berichtigt.

- **Rück-Übernahme wird still ignoriert.** `self.active` wird nie zurückgesetzt (`:2788`,
  `:2805`). Wird Tablet A von B verdrängt und drückt A wieder „Übernehmen", verwirft der
  Relay das `take_over` (`:2800`); die Seite zählt weiter
  (`src-tauri/assets/tablet.html:6987–6992`), und `is_holder` verwirft ihre Punkte.
- **Selbst-Verdrängung beim wiederholten `identify`.** Hält die Sitzung das Feld schon und
  sendet `identify` erneut (4-s-Retry, `tablet.html:2162–2167`), entfernt `attach_tablet`
  den eigenen Eintrag und schickt `SessionSuperseded` an den eigenen Kanal (`:3670–3677`).
  Das Tablet sperrt sich. Erreichbar, wenn nach dem Freiwerden binnen 4 s kein
  `MatchAssigned` kommt (Host weg).
- **Falscher Read-Idle im Host.** Nach einem Inline-Block von mehr als 15 s sind Lese-Arm
  und `idle_ticker` gleichzeitig bereit; `select!` wählt zufällig, und der Idle-Arm beendet
  die Sitzung selbst (`relay_client.rs:268–275`) — auch wenn der Relay den Host noch hält.
- **`monitor_upload` prüft den Host nicht**, nur ob der Namespace existiert (`:2625–2627`),
  entgegen dem eigenen Kommentar (`:2571–2572`); Base64 wird dekodiert, bevor der Namespace
  geprüft ist. `monitor_control_upload` ebenso (`:1343`).
- **Kein Status-Weg Relay-Verbindung → Dashboard.** `TabletInfo` trägt nur
  `cloud_enabled`/`relay_base`; der Zustand der Verbindung steht nur im Log.
  `docs/cloud-relay.md:1136–1137` behauptet, der echte Turnier-PC sehe die „Zweiter
  Host"-Warnung — sie steht nur im Relay-Log (`main.rs:4327`).
- **Die Dienst-Datei wird vom Deploy nicht ausgerollt.** `relay-deploy.yml` kopiert nur die
  Binary (`:72–80`). Eine Änderung an `ops/bts-relay.service` wirkt erst nach einem
  Handgriff am Server.
- **Die „Blende" am Feld-Monitor ist nur ein kleiner roter Text** unten links
  (`monitor.html:296–300`); der eingefrorene Stand bleibt voll sichtbar. `monitor.html`
  lädt seine Logs außerdem absolut nach `/pi-log` (`:687`), in der Cloud also an badhub.de.

## Zielbild & Erfolgskriterien

1. Kein Tablet und kein Fremder hinterlässt tote Feld-Slots; die Zahl der Feld-Schlüssel je
   Namespace ist begrenzt.
2. Jeder WebSocket-Weg hat einen Größen-, jeder Tablet-Weg einen Mengendeckel; kein Kanal
   wächst unbegrenzt. Ein legitimes Gerät merkt davon nichts.
3. Der Turnier-PC beantwortet Pings, während ein Ergebnis, ein Werbe-Upload oder ein
   Control-Sync hängt. Träges BTP wirft ihn nicht mehr ab.
4. Der Verbindungsaufbau hat eine Frist; nach einem Netzwechsel laufen die Cloud-Tablets
   nach etwa 20 s wieder. Der Turnierleiter liest am Dashboard, wenn ein anderer PC mit
   seinem Turnier verbunden ist.
5. Eine Cloud-Anzeige zeigt nach Verlust des Turnier-PCs den letzten Stand noch 20 s,
   danach „Keine Verbindung zum Turnier-PC". Ein kurzer Wackler bleibt unsichtbar, ebenso
   ein Relay-Neustart, wenn der Turnier-PC binnen 20 s zurück ist.
6. Der Sprachdienst-Schlüssel ist mit dem Abgang des Turnier-PCs aus dem Speicher, die
   Turnierdaten nach 60 min.
7. Der Relay-Dienst hat eine Speicher-Obergrenze; ein überlaufender Relay reißt badhub.de
   nicht mit.

Erfolg heißt außerdem: kein neuer Schalter, keine neue Einstellung, kein neues
Speicherformat. Für den Turnierleiter ändern sich genau zwei Dinge sichtbar — der Text auf
den Anzeigen bei Host-Verlust und eine neue Dashboard-Meldung.

## Nicht-Ziele

- **Host-Authentifizierung am Relay.** `host-ws` bleibt ohne Anmeldung; der Host-Slot ist
  mit Namespace-Kenntnis weiter übernehmbar (siehe „Sicherheit").
- **nginx- und Cloudflare-Limits** (`limit_conn`/`limit_req`, Cloudflare-Regeln). Ein Limit
  je IP träfe eine ganze Halle; eigener Ops-Schritt.
- **Das globale Schloss des Relays aufteilen** — Umbau ohne belegten Engpass.
- **`MAX_TL_WS_VORAUTH` je IP** — bräuchte erstmals IP-Vertrauen im Relay.
- **TL-Zustand nur bauen, wenn ein Cloud-Gerät zusieht** — bräuchte ein neues Signal
  Relay → Host.
- **Zettel über langsamen Uplink.** Ein 5-MB-Zettel über eine Leitung unter etwa
  2,8 Mbit/s wirft den Host weiter ab (gerechnet).
- **Host-Verlust auf der Tablet-Seite anzeigen.**
- **Speicherung auf Platte** — der Relay bleibt ein reiner RAM-Dienst.
- Fristen um `socket.send` im Relay; Feld-Schloss für den LAN-Ergebnisweg; Budget für
  Tablet-Zustände über alle Namespaces; Größen-Deckel am lokalen WebSocket der
  Slave-Brücke; Maskierung der Kennung im nginx-Zugriffslog.
- Paket 2 (badhub-Push) und die Einzelpunkte „Datensparsamkeit" und „Anzeigen und
  Sonstiges" des Audits.

Als offene Punkte in [roadmap.md](../roadmap.md) vermerkt sind: Host-Authentifizierung,
nginx-Limits als Ops-Schritt (samt nginx-Zugriffslog), `MAX_TL_WS_VORAUTH` je IP, TL-Zustand
nur bei Zuschauern, Zettel über langsamen Uplink, Host-Verlust am Tablet, Feld-Schloss für
den LAN-Ergebnisweg und das Zustands-Budget. Das globale Schloss steht dort schon als
Audit-Stichpunkt.

## Die vier Entscheidungen des Nutzers (11.11.2026)

| Frage | Entscheidung |
|---|---|
| Wie lange zeigt ein Hallen-TV nach Verlust des Turnier-PCs den letzten Stand? | **15–20 s weiter, dann die Meldung.** Umgesetzt als eine Konstante von 20 s. |
| Der Turnier-PC war noch nie verbunden? | **Gleiche Meldung** statt leerem Feld oder Werbung. Die **Kopplungsseite** eines unzugewiesenen Monitors **bleibt sichtbar**. |
| Wie lange bleiben Turnierdaten nach Abgang des Turnier-PCs im Relay? | **60 min.** Der **Sprachdienst-Schlüssel wird sofort** verworfen. Der gesicherte Tablet-Stand bleibt, solange ein Tablet am Feld hängt. |
| Schutz gegen massenhaft angelegte Schein-Turniere? | **Minimal, in diesem Paket:** Speicher-Obergrenze für den Dienst, Bilder erst nach einer Feldliste des Hosts, Gesamt-Budget für Bilder. **Keine Host-Anmeldung.** |

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:** `relay/src/main.rs` (Erweiterung, `router` wird aus `main`
  herausgelöst), `relay/Cargo.toml` (eine dev-dependency), `relay-proto/src/lib.rs`
  (Close-Codes), `src-tauri/src/tablet/relay_client.rs`, `carrier.rs`, neues
  `tablet/ws_verbindung.rs`, `tablet/state.rs`, `commands.rs`,
  `src-tauri/assets/{monitor,tafel,overview,combo,preparation}.html`,
  `src/components/AlertBanner.tsx`, `src/types.ts`, `ops/bts-relay.service`,
  `ops/nginx-bts-relay.conf`.
- **Architekturregeln:**
  - R1: Die Dashboard-Meldung kommt über das bestehende Command `tabletOverview`.
  - R3: Betroffen ist nur der Cloud-Pfad. Im LAN ändert sich nichts; im LAN+Cloud-Betrieb
    zählt das LAN bei Host-Verlust weiter.
  - R4 unverändert: ein Host je Namespace, ein aktives Tablet je Court. Die Halterschaft
    wird sogar strenger (Halter ist, wer in `tablets` steht).
  - R5 unverändert: `process_result` bleibt die einzige Ergebnis-Prüfung; nur die
    Reihenfolge der Aufrufe wird im Cloud-Pfad je Feld geordnet (ADR 0091).
  - R6: Das Relay-Log nennt nur noch die ersten 8 Zeichen der `install_id`; die Zuordnung
    zum Diagnose-Log bleibt möglich.
- **Konfiguration & Abwärtskompatibilität:** kein neues Config-Feld, kein gespeichertes
  Format. Jede Relay-Änderung läuft mit älteren Hosts, Seiten und Kiosks; jede
  Host-Änderung mit altem und neuem Relay (Matrix unter „Deploy-Regeln"). `identifier` und
  Updater-Pfad bleiben unangetastet.
- **Datenschutz:** eine Verbesserung — Namen, Bilder und Schlüssel liegen nach Host-Abgang
  nicht mehr unbegrenzt im RAM. Kein neues personenbezogenes Feld. Die Frist gehört in den
  Abschnitt „Sicherheit" von `docs/cloud-relay.md`; ein neuer Abschnitt in `CLAUDE.md` ist
  nicht nötig.
- **Abhängigkeiten:** keine neue Laufzeit-Abhängigkeit (Fristen über tokio, Zufall für den
  Jitter aus `std`). Eine **dev-dependency** im Relay: `tokio-tungstenite` 0.29 — liegt
  über axum bereits im Lockfile, gleiche Lizenz und Pflege wie die Laufzeit-Abhängigkeit
  der App.

## Lösung

Die Buchstaben folgen dem How-To. Feld-Schlüssel und Halterschaft (a) sind Fehlerbehebung
ohne echte Wahl und bekommen deshalb keinen eigenen ADR.

### a) Tote Slots, Halterschaft, Feld-Schlüssel (PR 3)

**Regel für `identify`:**

| Lage der Sitzung | Verhalten |
|---|---|
| noch kein Feld | wie heute |
| gleiches Feld, Sitzung hält es | nichts tun; kein `SessionSuperseded`, kein zweites `TabletConnected` |
| gleiches Feld, Sitzung hält es nicht | normaler Versuch (der 4-s-Retry bei „belegt") |
| anderes Feld | verwerfen, einmal je Sitzung melden, Verbindung bleibt |

Ein Feldwechsel ist immer ein Neuladen mit neuem Socket; ein legitimes Gerät trifft die
letzte Zeile nie.

**Halterschaft.** `self.active` entfällt als Wahrheit. Halter ist, wer in `tablets` steht
(`is_holder`). `TakeOver` gilt, wenn die Sitzung ein Feld hat und es gerade nicht hält.
`forward_battery` bekommt die Halter-Prüfung. `device_id` wird auf 128 Zeichen gekürzt.
`take_over_court` legt nichts mehr ohne Limit an (gemeinsamer Helfer „holen oder mit Limit
anlegen").

**Feld-Schlüssel.**

- Ein Feld heißt **host-bekannt**, wenn es in `courts`, `court_matches` oder `court_labels`
  steht.
- Ein Tablet darf Einträge nur für ein Feld erzeugen, das host-bekannt ist oder das es
  gerade hält. Host-unbekannte Schlüssel sind damit durch `MAX_TABLETS_PER_NS` (64)
  begrenzt.
- `detach_tablet` räumt **nur für host-unbekannte Felder** `court_scores`, `court_state`,
  `display_court_state`, `monitor_seq`, `vor_match`. Für host-bekannte Felder bleibt alles
  wie heute — der gesicherte Stand gehört dem Ersatz-Tablet.
- Neuer Deckel `MAX_FELD_SCHLUESSEL_JE_NS = 128` für die Zahl verschiedener CourtIDs über
  alle Feld-Karten, geprüft bei Tablet- und Host-Frames. Er hängt nicht an der Feldliste
  (die ist nach einem Relay-Neustart leer). Überschreitung: verwerfen, einmal je
  Host-Sitzung melden.

Bewusst in Kauf genommen: Zählt ein Cloud-Tablet nach einem Relay-Neustart ohne Host und
fällt dann aus, bekommt das Ersatzgerät vom Relay keinen Stand. Sobald der Host zurück ist,
greifen Host-Sätze und Rückspiegel.

### b) Deckel (PR 4), begrenzte Kanäle (PR 5)

**Zwei Arten von Deckeln** (ADR 0090): *Fachliche* Deckel verwerfen oder kürzen und melden
einmal — die Verbindung bleibt. *Transport*-Deckel sind reiner Speicherschutz und trennen.

**Fachliche Deckel:**

| Frame / Route | Deckel | Verhalten |
|---|---|---|
| Tablet `score_update.sets_history` | ≤ 10 Sätze | Frame verwerfen, einmal je Sitzung melden |
| Tablet `identify.device_id` | 128 Zeichen | kürzen |
| Tablet `state_sync` | 64 KiB | wie heute (verwerfen, einmal melden) |
| Tablet `rally_sync` / `match_event_sync` | wie heute (8 KiB / 16 KiB, 64 Stück) | wie heute, zusätzlich einmal je Sitzung melden |
| Tablet Menge | Eimer je Sitzung (unten) | verwerfen, einmal melden |
| Träger `frame.payload` | 160 KiB | wie heute verwerfen, zusätzlich einmal je Substrom melden |
| Host `MatchAssigned` | Brief ≤ 16 KiB serialisiert · Label/Halle ≤ 128 Zeichen · `host_sets` ≤ 10 · Feld-Schlüssel ≤ 128 | Brief zu groß oder Schlüssel-Deckel: Frame verwerfen und melden · Label/Halle kürzen · zu viele `host_sets`: leer durchreichen |
| Host `MatchCleared` | Label/Halle ≤ 128 Zeichen | kürzen; das Räumen wirkt immer |
| Host `Courts` | 512 Einträge (heute) · Label/Halle ≤ 128 · Farbe ≤ 16 · `azure_tts` ≤ 8 KiB | kürzen; ein zu großer Schlüssel-Block wird zu `None` und gemeldet |
| Host `ScoreUpdate` | Sätze ≤ 10 (heute) · Feld-Schlüssel ≤ 128 | verwerfen und melden |
| Host `Prepared` | 200 Einträge (heute) · gesamt ≤ 256 KiB | Liste leeren und melden (ehrliche Fehlanzeige wie beim TL-Zustand) |
| Host `TlAuth` | 64 Geräte (heute) · Zugang/Kennung/Profil ≤ 256 Zeichen | Frame verwerfen und melden |
| Host `TlState` | `MAX_TL_STATE_LEN` statt `MAX_STATE_LEN` | gleicher Wert (64 KiB), richtige Konstante |
| `POST /{ns}/monitor` | Turniername ≤ 256 Zeichen · Konfiguration ≤ 16 KiB | kürzen bzw. Vorgabe einsetzen, melden |
| `POST /{ns}/monitor/control` | je Karte ≤ 256 Einträge · Schlüssel ≤ 64 Zeichen | 413, einmal melden |

Der Brief-Deckel (16 KiB) und der Schlüssel-Deckel (128) sind geschätzt; vor dem Festlegen
wird ein echter Brief mit Doppel, Schiedsrichtern und Zähltafelbediener gemessen (Annahme:
unter 4 KiB).

**Transport-Deckel** über `max_message_size` und `max_frame_size` am Upgrade:

| Upgrade | Deckel | Begründung |
|---|---|---|
| Tablet | 1 MiB | größter legitimer Frame 64 KiB; selbst ein Format-1-Zustand von 590 KB bleibt als Nachricht bei rund 740 KB |
| Monitor, TL | 64 KiB | die Seiten senden nichts bzw. einen Zugangs-Frame bis 4 KiB |
| Träger | 4 MiB | ein 590-KB-Zustand einer alten Seite wird im Umschlag zweimal escaped: 590 + 3 × 150 ≈ 1040 KB — 1 MiB würde die ferne Halle bei jedem Punkt trennen |
| Host | `2 × MAX_SCORESHEET_HTML_LEN + 64 KiB` (= 10 432 KiB, rund 10 MiB) | der Relay verwirft einen zu großen Zettel erst nach dem Empfang; der Transport-Deckel muss über dem größten fachlich zulässigen Frame samt JSON-Escaping liegen |

Der Host-Deckel wird per Compile-Zeit-Zusicherung an den fachlichen gebunden. Beim
Überschreiten sendet die Schleife nach Möglichkeit `Close(1009)` und schreibt eine Warnung;
ein alter Client sieht einen Abriss und verbindet neu.

**Mengen-Takt je Tablet-Sitzung** (und damit je Substrom des Trägers):

| Klasse | Schwung | Nachwuchs |
|---|---|---|
| Score, Rally, Battery, Alert, Einzel-Ereignis | 60 | 20 je Sekunde |
| `rally_sync`, `match_event_sync` | 4 | 0,5 je Sekunde |
| Weitergabe von `state_sync` | 6 | 2 je Sekunde (heute) |

Legitime Spitze: Die Seite sperrt 0,7 s je Punkt (`tablet.html:4054`), je Punkt gehen etwa
vier Frames — höchstens rund 6 je Sekunde. Ein Voll-Sync nach Abriss sind etwa fünf Frames
auf einmal. `ping` zählt nicht mit. Der Eimer ist eine reine Funktion und löst
`host_weitergabe_erlaubt` ab.

**Begrenzte Kanäle (PR 5).** `Tx` wird ein kleiner Typ um `mpsc::Sender` mit `try_send` und
einem Überlauf-Signal. Ist der Kanal voll, wird das Signal gesetzt; die Verbindungsschleife
des **Empfängers** hat dafür einen eigenen `select!`-Arm, meldet einmal und beendet sich
über ihren normalen Aufräumweg. Nichts wird still verworfen.

| Kanal | Größe |
|---|---|
| Host | 1024 |
| Tablet, Substrom | 64 |
| Monitor, TL | 32 |
| Träger-Sammelkanal | 1024 |

1024 beim Host reicht für rund 100 s Spitzenverkehr einer Halle und damit auch für einen
alten Host, der bis zu 35 s inline wartet. Ein `MatchAssigned` geht nicht verloren: Das
getrennte Tablet meldet sich neu an, der Relay schickt `TabletConnected`, der Host schiebt
die Zuweisung nach (`relay_client.rs:494–499`).

### c) Host-Schleife (PR 1)

Die Verbindung wird in Leser und Schreiber geteilt.

- **Schreiber-Task** besitzt den Sink, sendet aus dem Ausgangskanal mit 20 s Frist je
  Nachricht und meldet ein Scheitern an die Schleife.
- **Schleife** liest, verteilt und tickt; sie wartet auf nichts Langes mehr. Pongs gehen
  sofort in den Ausgang. Der Idle-Wächter schlägt nur an, wenn wirklich nichts ankam.
- **Ergebnis-Arbeiter je Feld** (ADR 0091): eine Warteschlange mit 4 Plätzen und ein Task
  je CourtID, angelegt beim ersten Ergebnis. Er gehört `run`, nicht der Sitzung.
  - Reihenfolge je Feld strikt nach Eingang; Felder laufen nebeneinander.
  - Der 8-s-Retry des Tablets reiht sich hinter den ersten Versuch und trifft danach die
    Idempotenz (`settled_ok`, ADR 0018) — kein zweiter BTP-Write.
  - Reißt die Verbindung mitten im Write ab, läuft der Write zu Ende. Die Quittung geht an
    den Sende-Kanal der Sitzung, aus der der Auftrag kam; ist sie beendet, verpufft sie.
    **Eine Quittung geht nie über eine neue Sitzung** — der Relay zählt `req_id` je
    Namespace ab 1 (`main.rs:638`); nach einem Relay-Neustart könnte eine alte Nummer ein
    fremdes Ergebnis mit „ok" quittieren.
  - Volle Warteschlange: sofortige, wiederholbare Absage.
  - `process_result` bleibt unverändert (R5).
- **Werbe-Upload und Control-Sync** laufen als eigene Tasks mit je einem „läuft
  schon"-Wächter, der `run` gehört (höchstens einer zugleich, auch über Sitzungen). Der
  Fingerabdruck wird nur bei Erfolg gesetzt und bleibt je Sitzung. Der Upload bekommt 120 s
  Frist statt 15 s und nach einem Fehlschlag wachsende Pausen (30 s bis 5 min).
- **Zettel** wandern in `spawn_blocking`. Timeline und Schiedsrichter-Detail bleiben inline
  (klein, kein BTP), ebenso `handle_score` (Reihenfolge je Feld).
- **Testbarer Kern:** `serve` wird zu „verbinden" plus `sitzung_fahren`, generisch über den
  Strom und mit injizierbarem Ergebnis-Verarbeiter. Tests fahren ihn über
  `tokio::io::duplex` mit echtem WebSocket-Rahmen, ohne Netz.

### d) Verbindungsaufbau (PR 2, Meldung in PR 9)

**Frist:** 10 s um `connect_async` (DNS, TCP, TLS, Upgrade), in `relay_client.rs` und
`carrier.rs` über einen gemeinsamen Helfer in `tablet/ws_verbindung.rs`.

**Zustandsautomat des Backoff** (reine Funktion, Zufall als Parameter):

| Ausgang der Sitzung | Nächste Pause |
|---|---|
| stand mindestens 30 s | zurück auf 1 s |
| Aufbau gescheitert (Netz, Frist, 502) | verdoppeln bis 30 s |
| Close-Grund „Host belegt" | verdoppeln, höchstens 5 s |
| sauberer Close binnen 30 s ohne Grund | verdoppeln, höchstens 5 s |
| Abriss binnen 30 s | verdoppeln bis 30 s |
| Abriss in den ersten 5 s (aus dem Review von PR 2) | wie Close ohne Grund — der Close-Frame einer Abweisung kann verloren gehen |

Die vierte Zeile deckt den alten Relay, der ohne Grund schließt. Jede Pause bekommt
± 20 % Jitter.

**Close-Grund (PR 7):** Der Relay schließt bei belegtem Host-Slot mit Code 4409 und Text
`host_belegt`, beim Namespace-Limit mit 4429 und `voll`. Ein alter Host wertet den
Close-Frame nicht aus (`relay_client.rs:205`).

**Dashboard (PR 9):** `TabletInfo` bekommt ein Feld `cloud_lage`, gespeist aus einem Merker
im `TabletState`, den `relay_client` setzt (mindestens: verbunden, nicht verbunden,
abgewiesen seit wann). `AlertBanner.tsx` fragt `tabletOverview` schon alle 3 s ab. Die
Meldung **„Ein anderer PC ist mit diesem Turnier verbunden"** erscheint erst, wenn die
Abweisung länger als 30 s anhält — nach einem Netzwechsel ist die eigene tote Verbindung
bis zu 15 s lang der „andere PC".

**Träger (`carrier.rs`, PR 2):** dieselbe Frist und derselbe Automat (Rücksetzen bei
`Ready` plus 30 s Stabilität), 20 s Sende-Frist, `frame()` verwirft bei nicht bereitem
Träger, Rest-Nachrichten der alten Sitzung werden vor dem `Hello` geleert, Nutzlast über
160 KiB wird schon im Slave verworfen und einmal gemeldet.

### e) Host-Verlust auf den Anzeigen (PR 7, ADR 0088)

**Eine Uhr:** `host_weg_seit` im Namespace.

- Gesetzt beim Freigeben des Slots auf das **letzte Lebenszeichen** der Verbindung. Ein
  stumm gestorbener Host ist damit rund 20 s nach seinem letzten Lebenszeichen gesperrt,
  nicht nach 15 + 20 s.
- Gelöscht bei erfolgreicher Übernahme durch `try_claim_host`.
- Ein Namespace, den ein Tablet anlegt, startet mit „weg seit jetzt".

**Der Namespace überlebt den Host-Abgang.** `is_empty` zählt einen Namespace mit laufender
Uhr als nicht leer. Sonst gäbe es im LAN+Cloud-Betrieb ohne Cloud-Tablets nichts, was 20 s
weitergezeigt werden könnte. Das Entfernen übernimmt ein **Sweeper** (ein Task ab `main`,
alle 5 s): in PR 7 nach Ablauf der Karenz, ab PR 8 nach 60 min (f).

**Lage** (reine Funktion):

| Zustand | Antwort der Stand-Routen |
|---|---|
| Host verbunden | normal |
| Host weg, unter 20 s | 200 mit dem gehaltenen Stand |
| Host weg, ab 20 s | gesperrt: 503 |
| Namespace fehlt | gesperrt: 503 — außer in der Start-Karenz |

**Start-Karenz.** In den ersten 20 s nach Prozessstart beantwortet der Relay Abrufe für
einen fehlenden Namespace mit 304, wenn sie `If-None-Match` tragen, sonst mit 503. Der
Relay hat nach dem Neustart keinen Stand mehr, die Seite hat ihn noch. Ein Relay-Neustart
bleibt so auf laufenden Fernsehern unsichtbar, wenn der Host binnen 20 s zurück ist
(üblich: 1–4 s).

**Routen:**

| Route | Verhalten ohne Host |
|---|---|
| `GET /{ns}/court/{id}/state` (auch `?zweck=stream`) | 503 nach Karenz |
| `GET /{ns}/monitor/state?device=` | 503 nach Karenz, wenn das Gerät ein bekanntes Ziel hat; sonst 200 Kopplungsseite mit neuem Feld `hostOffline` |
| `GET /{ns}/health` (voll und `?court=`) | 503 nach Karenz |
| `GET /{ns}/info/preparation/state` | 503 nach Karenz |
| `GET /{ns}/meetingpoint/state` | 503 nach Karenz (statt 404 „aus") |
| `GET /{ns}/info/announce/state` | **503 sofort** |
| `GET /{ns}/courts`, `/info/ad/state`, `/ads/*`, `/info/logo`, HTML-Seiten, `monitor-devices`, `slaves` | unverändert |

- `announce/state` sperrt sofort: Ein 200 ohne Schlüssel liest der Ansage-Slave als „der
  Turnier-PC hat den Sprachdienst abgeschaltet" und verwirft seine geerbte Stimme
  (`commands.rs:5486–5499`); einen Fehler übergeht er.
- `/courts` braucht das Tablet für den Feldwechsel; `/info/ad/state` ist keine
  Stand-Anzeige.
- Fehlt der Namespace, kennt der Relay keine Zuweisung. Jedes Gerät gilt dann als
  unzugewiesen und sieht nach der Start-Karenz die Kopplungsseite mit dem Hinweis — die
  Folge der Entscheidung „Kopplungsseite bleibt sichtbar".

**Reihenfolge in der Route.** Die Lage wird unter dem Schloss **vor** Cache-Blick, Bau und
Markenvergleich entschieden. Gesperrt gibt es weder 304 noch Cache-Treffer noch Ablage. Das
Lebenszeichen des Geräts (`monitor_seen`) wird weiter vorher gestempelt. Die 503-Antwort
trägt `Cache-Control: no-store`, `Retry-After: 2` und den Rumpf `{"grund":"host_weg"}` —
den Rumpf statt eines Kopfes, weil die Slave-Brücke eigene Köpfe nicht durchreicht.

**Anstöße:**

- Host-Abgang: `tl_subs` schließen (die TL-Seite fällt auf ihren Poll und sieht 503; ein
  toter TL-Tab hält den Namespace nicht mehr). Monitor-Abos bleiben.
- Karenz-Ende (Sweeper): einmal alle Monitor-Abos anstoßen, damit die Meldung sofort
  erscheint statt erst beim 4-s-Sicherheitsabruf.
- Rückkehr nach Karenz-Ende: einmal alle Monitor-Abos anstoßen.

`/health` (prozessweit) nennt zusätzlich die Zahl verbundener Hosts — der Prüfpunkt vor
jedem Merge.

**Seiten:**

| Seite | Nach 503 heute | Änderung |
|---|---|---|
| `monitor.html`, `tafel.html` | kleiner roter Hinweis, Stand bleibt, zwei Log-Uploads je Abriss | bei `grund = host_weg`: Text „Keine Verbindung zum Turnier-PC", **Stand abdunkeln**, kein Log-Upload (nur Eintrag im Puffer); `hostOffline` auf der Kopplungsseite anzeigen |
| `overview.html`, `combo.html`, `preparation.html` | Banner „Keine Verbindung zum Turnier-PC" ab dem 2. Fehler | bei `host_weg` schon beim ersten 503 |
| `stream.html` | hält den Stand, nach 60 s ohne Stand durchsichtig | keine — in OBS erscheint nie ein Fehlertext; durchsichtig rund 80 s nach Host-Verlust |
| `ad.html` | Route bleibt 200 | keine |
| `meetingpoint.html` | „Turnier-PC antwortet nicht" | keine |
| `tablet.html` | ignoriert | keine (Nicht-Ziel) |
| `tl.html` | 503 schon heute | keine |
| Slave-Brücke | reicht Status und Rumpf durch | keine |

Alte Seiten (ferne Halle mit alter App, seit Tagen offene Kiosk-Seiten) behandeln 503 wie
jeden Fehler: Das Verhalten ist richtig, nur Wortlaut und Log-Uploads bleiben alt.
`combo.html` ist nicht im Relay eingebettet; ihre Änderung wirkt erst mit dem App-Release.

### f) Gestaffelte Lebensdauer (PR 8, ADR 0089)

Der Sweeper geht alle 5 s unter dem Schloss durch alle Namespaces und wendet eine reine
Entscheidungsfunktion an.

| Zeitpunkt | Was geht |
|---|---|
| **sofort** beim Host-Abgang | `azure_tts`; wie heute TL-Zugänge, TL-Zustand, wartende Abrufe; `tl_subs` geschlossen (ab PR 7) |
| **nach 20 s** | Anzeigen gesperrt, Anstoß (PR 7) |
| **nach 60 min** | `court_matches` (Felder ohne Tablet), `prepared`, `meetingpoint`, `courts`, Labels, Hallen, Freitext, `monitor` (Bilder, Logo, Konfiguration), `monitor_control`, `monitor_seen`, `slaves`, `court_on_court_since`, `vor_match`, Übersichts-Cache; für Felder ohne Tablet auch `court_scores`, `court_state`, `monitor_seq` |
| **solange ein Tablet am Feld hängt** | `tablets`, `tablet_devices`, `court_state`, `court_scores` und ein **anonymisierter Brief** (nur `match_id` und `finalized`) |
| **danach** | Namespace entfernen, sobald kein Tablet und kein offenes Ergebnis mehr hängt; Monitor-Abos bekommen wie heute ein Close |

Der anonymisierte Brief ist nötig: Ohne `court_matches`-Eintrag verwirft der Stale-Filter
jeden weiteren Punkt des Tablets (`:3982–3990`). Mit ihm gilt bei der Rückkehr des Hosts
„gleiches Spiel", und Sätze und Zustand werden nicht geräumt — kein 0:0.

Wechselwirkungen:

- **Innerhalb der 60 min** ist nichts geräumt; die Rückkehr ist der heutige Reconnect-Pfad.
- **Nach dem Leeren, Feld ohne Tablet:** wie nach einem Relay-Neustart — der Host liefert
  die Sätze, der Rückspiegel einmal je Verbindung den Zustand.
- **`monitor_fp`** des Hosts wird nicht falsch: Geleert wird nur ohne Host, jede neue
  Sitzung beginnt mit leerem Fingerabdruck; Leeren und Übernahme laufen unter demselben
  Schloss.
- **`MAX_NAMESPACES`:** Ist die Tabelle voll, wird der am längsten hostlose Namespace ohne
  Tablet verdrängt. Gehaltene Daten können nie einen neuen Host aussperren.

### g) Mindestschutz gegen Schein-Turniere (PR 6, ADR 0090)

- **Speicher-Obergrenze des Dienstes:** `MemoryHigh=512M`, `MemoryMax=768M`,
  `MemorySwapMax=0` in `ops/bts-relay.service`. Herleitung: 20 gleichzeitige Turniere mit
  Bildern bis zum Gesamt-Budget (256 MiB), Zuständen (20 × 3,3 MiB), zwei laufenden Uploads
  (je rund 50 MiB vorübergehend) und Grundlast ergeben knapp 480 MiB; der Server hat rund
  1,5 GB frei. Beim Erreichen beendet der Kernel den Dienst, systemd startet ihn nach 3 s
  neu — das wirkt wie ein Deploy auf alle Turniere, schützt aber badhub.de.
- **Gesamt-Budget für Bilder:** ein prozessweiter Zähler über alle Werbe- und Logo-Bytes,
  Grenze 256 MiB. Darüber nimmt der Upload die Konfiguration an, verwirft die Bilder und
  antwortet 507 mit einer Log-Zeile. Der Zähler sinkt beim Ersetzen und Entfernen.
- **Bilder-Tor:** `POST /{ns}/monitor` prüft **vor dem Lesen des Rumpfs**, ob der Host
  verbunden ist und in dieser Sitzung ein `Courts`-Frame geschickt hat.
  - Ganz ohne Host: 409, ohne den Rumpf zu lesen.
  - Host da, noch kein `Courts`-Frame: Konfiguration, Turniername und Aufruf-Timer
    annehmen, Bilder verwerfen, 409.
  - **Jedes `Courts`-Frame zählt, auch ein leeres** — sonst zeigte ein Turnier-PC ohne
    geladenes BTP-Turnier in der Cloud keine Werbung mehr.
  - `POST /{ns}/monitor/control` bekommt dieselbe Host-Prüfung (409 ohne verbundenen Host).
- **Alter Host:** Sein erster Upload nach jedem Verbinden kommt vor seiner Feldliste an. Er
  protokolliert eine Warnung und versucht es nach 30 s erneut, dann mit Erfolg. Sichtbar
  ist das nur nach einem Relay-Neustart: 30 s ohne Werbung. Bei einem Host-Wackler sind die
  alten Bilder noch da.
- **Neuer Host (PR 9):** sendet beim Verbinden immer ein `Courts`-Frame und wiederholt den
  Upload bei 409 nach 3 s.

### h) Log (PR 6)

- **Verfall:** `RollingFileAppender::builder()` mit Tagesrotation und höchstens 30 Dateien;
  scheitert der Bau, bleibt stdout.
- **Gekürzte Kennung:** An allen Stellen, die heute `'{ns}'` schreiben, stehen nur die
  ersten 8 Zeichen. Wer die volle `install_id` aus dem Diagnose-Log kennt, sucht im
  Relay-Log nach ihren ersten 8 Zeichen (Kollisionswahrscheinlichkeit bei 2000 Namespaces
  etwa 0,05 %). Das Rezept gehört nach `docs/logging.md`. Ein Wächter-Test hält `'{ns}'`
  aus dem Quelltext.

## Deploy-Regeln

**Was einen Relay-Deploy auslöst.** Jeder main-Merge, der `relay/**`, `relay-proto/**`,
eine im Relay eingebettete Seite (`tablet`, `monitor`, `tl`, `overview`, `preparation`,
`ad`, `tafel`, `stream`, `anzeige`, `meetingpoint`) oder das Flaggen-Verzeichnis berührt
(`.github/workflows/relay-deploy.yml:13–41`). **Jeder Deploy leert den RAM und trennt alle
Verbindungen.**

| PR | Löst Deploy aus | Grund |
|---|---|---|
| 1, 2, 9 | nein | nur `src-tauri/src/**` und `src/**`; `relay-proto` bleibt unberührt |
| 3, 4, 5, 6, 8 | ja | `relay/**` |
| 7 | ja | `relay/**`, `relay-proto/**`, eingebettete Seiten |

**Merge-Fenster.**

- Die sechs Relay-PRs gehen in **zwei Fenstern**: erst 3, 4, 6 (klein, voneinander
  unabhängig), dann 5, 7, 8 (5 ist der größte mechanische Umbau; 7 und 8 teilen Uhr und
  Sweeper, 8 baut auf 7).
- Montag bis Donnerstag, nie an einem Turnier-Wochenende.
- **Vor jedem Merge `/health` des Relays prüfen:** `tablets == 0`, ab PR 7 zusätzlich
  `hosts == 0`. Mehrere Verbände nutzen den Relay; ein leeres `/health` ist ein Indiz, kein
  Beweis.
- **Zwischen zwei Relay-Merges den Deploy-Lauf abwarten und prüfen.** Der Rückweg
  `bts-relay.rollback` reicht nur einen Schritt zurück; nach zwei Merges an einem Abend ist
  der Vor-Vorgänger nur per Revert und neuem Merge erreichbar.
- PR 7 berührt `relay-proto`; ein gemeinsames Fenster mit einem Merge aus Paket 2 spart
  einen Neustart.

**Handgriff für `ops/bts-relay.service` (im Fenster von PR 6).** Die Datei wird nicht
automatisch ausgerollt.

1. Am Server den heutigen Speicherverbrauch des Dienstes ablesen
   (`systemctl status bts-relay`). Liegt er über 256 MiB, die Werte vor dem Setzen neu
   herleiten.
2. Die Unit nach der Anleitung in `docs/cloud-relay.md` (Abschnitt „Deployment") auf den
   Server bringen.
3. `sudo systemctl daemon-reload`, danach `sudo systemctl restart bts-relay` — der Neustart
   zählt wie ein Deploy und gehört deshalb in dasselbe Fenster.
4. Prüfen: `systemctl show bts-relay -p MemoryMax` nennt den Wert; `/health` antwortet.

Zurück: Zeilen entfernen, `daemon-reload`, Neustart.

**Handgriff für die nginx-Vorlage.** `ops/nginx-bts-relay.conf` bekommt nur die Zeile, die
am Server schon gilt: `client_max_body_size 25m;`. **Am Server ist nichts zu ändern**, kein
`nginx reload`. Limits (`limit_conn`, `limit_req`) kommen ausdrücklich nicht in die
Vorlage.

**Kompatibilität:**

| PR | alter Host × neuer Relay | neuer Host × alter Relay | alte Seiten, Kiosks, Slave-Brücke |
|---|---|---|---|
| 1, 2 | – | läuft; ein Close ohne Grund zählt als Abweisung (Pause höchstens 5 s) | – |
| 3 | unverändert | – | alte Tablet-Seite: der Retry für dasselbe Feld bleibt erlaubt |
| 4 | alle Frames liegen unter den Deckeln; der Zettel-Deckel ist aus dem fachlichen abgeleitet | – | Format-1-Seite hinter dem Träger: der Umschlag passt in 4 MiB |
| 5 | 1024 Plätze reichen für 35 s Inline-Wartezeit | – | ein getrenntes Gerät verbindet neu |
| 6 | Bilder 30 s später nach einem Relay-Neustart, Warnung im Host-Log | – | – |
| 7 | der Close-Grund wird ignoriert | läuft ohne Grund | 503 wird wie jeder Fehler behandelt; `hostOffline` ist unbekannt und folgenlos |
| 8 | Rückkehr wie nach einem Relay-Neustart | – | – |
| 9 | – | keine Dashboard-Meldung, sonst gleich; 409 kommt vom alten Relay nie | – |

**Rollback.** Relay: `bts-relay.rollback` (ein Schritt), sonst Revert und neuer Merge. Host:
Revert und neuer Tag; jeder Host-PR läuft mit altem und neuem Relay. Dienst-Datei: siehe
oben.

## Sicherheit — was nach diesem Paket noch geht

Der Relay bleibt ein offener Dienst ohne Host-Anmeldung. Ehrlich aufgeschrieben:

**Ohne Kenntnis eines Namespace** kann ein Fremder weiterhin

- bis zu 2000 Schein-Namespaces anlegen, indem er ebenso viele Host-Verbindungen offen
  hält. Ein verbundener Schein-Host hält seinen Namespace legitim; die Verdrängung in (f)
  trifft nur hostlose. Ist die Tabelle so gefüllt, werden neue Turnier-PCs abgewiesen.
- das Bilder-Budget füllen (je Schein-Turnier ein `Courts`-Frame, dann Bilder). Danach
  bekommen auch echte Turniere 507 für ihre Werbung; Spielbetrieb und Stände sind nicht
  betroffen.
- mit rund 15 voll bestückten Schein-Turniern die Speicher-Obergrenze erreichen. Der Dienst
  startet neu — für alle Turniere wie ein Deploy. badhub.de bleibt unberührt.

Nicht mehr möglich: tote Slots hinterlassen, einzelne Nachrichten bis 64 MiB einliefern,
Satzlisten und Feldnummern unbegrenzt speichern, Bilder ohne verbundenen Host hochladen,
den Server in den Speichermangel treiben.

**Mit Kenntnis eines Namespace** (steht in jeder Tablet- und Monitor-Adresse und auf dem
QR-Aushang) kann ein Fremder weiterhin

- **den Host-Slot übernehmen**, sobald der echte Turnier-PC 15 s still ist oder nach einem
  Relay-Neustart das Rennen verliert. Er kann dann Ergebnisse mit „ok" quittieren (das
  Tablet löscht sein gepuffertes Ergebnis), Tablets falsche Spiele zuweisen und eigene
  TL-Zugänge setzen. Das ist die **dokumentierte Abwägung** aus dem Zombie-Fix
  (`docs/cloud-relay.md`, Abschnitt „Sicherheit") und bleibt bestehen. Dieses Paket macht
  sie seltener (der Host wird durch träges BTP nicht mehr abgeworfen) und sichtbar (die
  Dashboard-Meldung am echten Turnier-PC).
- den Sprachdienst-Schlüssel lesen, solange der Turnier-PC verbunden ist (gewollt, ADR
  0003), Werbebilder und Monitor-Steuerung überschreiben, solange der Turnier-PC verbunden
  ist, alle Felder als Tablet übernehmen und Stände und Namen über die Anzeige-Routen
  lesen.

Die eigentliche Gegenmaßnahme ist eine Host-Anmeldung; sie ist ein Nicht-Ziel und steht in
der Roadmap.

## Zur Freigabe ausdrücklich vorgelegt

Diese Punkte wurden nach Empfehlung gesetzt, nicht vom Nutzer einzeln entschieden. Mit der
Freigabe der Spec gelten sie als bestätigt.

**Präzisierungen aus dem How-To (Nachtrag)**

1. **Das Turnier bleibt im Relay stehen, wenn der Turnier-PC geht.** Sonst gäbe es in Hallen
   ohne Cloud-Tablets nichts, was der Fernseher 20 s weiterzeigen könnte.
2. **Beim Abmelden eines Tablets wird nur aufgeräumt, was der Turnier-PC nicht kennt.** Der
   gesicherte Stand eines echten Felds bleibt für das Ersatz-Tablet — sonst käme das 0:0
   zurück.
3. **Die Obergrenze für Nachrichten des Turnier-PCs liegt bei rund 10 MiB statt 8 MiB.**
   Sie ist aus der Zettel-Obergrenze abgeleitet; knapper bemessen würde ein großer
   Sammeldruck den Turnier-PC vom Relay trennen.
4. **Die Sammelverbindung der fernen Halle darf 4 MiB je Nachricht tragen statt 1 MiB.**
   Ältere Tablet-Seiten dort schicken größere Stände; 1 MiB würde die Halle bei jedem Punkt
   trennen.
5. **Die Ansage-Abfrage meldet den fehlenden Turnier-PC sofort**, nicht erst nach 20 s.
   Sonst hielte der Ansage-PC der fernen Halle den Sprachdienst für abgeschaltet.
6. **Feldliste und Werbe-Zustand bleiben abrufbar.** Das Tablet braucht die Feldliste für
   den Feldwechsel; Werbung ist kein Spielstand.
7. **Für die Werbebilder genügt jede Feldliste des Turnier-PCs, auch eine leere.** Sonst
   gäbe es ohne geladenes BTP-Turnier keine Werbung in der Cloud.
8. **Ein Neustart des Relays bleibt 20 s lang unsichtbar.** Der Fernseher behält seinen
   Stand, bis der Turnier-PC wieder da ist; ohne diese Regel blinkte bei jedem Update des
   Relays jede Anzeige.
9. **Feld-Monitor und Zähltafel dunkeln den Stand ab und zeigen „Keine Verbindung zum
   Turnier-PC".** Der kleine rote Hinweis von heute lässt einen eingefrorenen Stand aktuell
   aussehen. In diesem Fall lädt die Anzeige kein Diagnose-Log hoch.
10. **Volle Warteschlangen trennen das langsame Gerät, statt Nachrichten wegzuwerfen;
    Ergebnisse laufen je Feld nacheinander.** Ein getrenntes Gerät verbindet neu und
    bekommt alles frisch; eine still verlorene Zuweisung dagegen friert ein Tablet ein.
11. **Große Zettel über eine sehr langsame Leitung bleiben ein offener Punkt.** Das Paket
    beseitigt die häufigen Ursachen; der Rest betrifft Sammeldrucke über schwache
    Leitungen.
12. **Neun kleine Schritte statt vier.** Zuerst zwei für den Turnier-PC (ohne
    Relay-Neustart), dann sechs für den Relay in zwei Wartungsfenstern, zuletzt die
    Dashboard-Meldung. Jeder ist einzeln zurückzunehmen.
13. **Vier Architektur-Entscheidungen (ADR 0088–0091).** Die Regeln zu Feld-Schlüsseln und
    Halterschaft sind Fehlerbehebung ohne echte Wahl und stehen nur hier.

**Schon früher gesetzt**

14. **Bei Abweisung fragt der Turnier-PC höchstens alle 5 s nach.** So laufen die
    Cloud-Tablets nach einem Netzwechsel nach etwa 20 s wieder; der Relay nennt den Grund,
    damit die App „ein anderer PC ist verbunden" zeigen kann.
15. **Zu große Nachrichten werden verworfen und einmal gemeldet; die Verbindung bleibt.**
    Nur offenkundiger Missbrauch (weit über jeder echten Größe) trennt — sonst hinge ein
    Gerät in einer Verbinde-Schleife.
16. **Werbe-Upload und Monitor-Steuerung laufen neben dem Spielbetrieb**, höchstens einer
    zugleich. Ein hängender Upload hält keine Punkte mehr auf.
17. **Keine nginx-Limits.** Ein Limit je Internet-Adresse träfe eine ganze Halle, und zwar
    genau im Störfall. Nur die Vorlage im Repo wird an den Server angeglichen.
18. **Vom Kleinkram bleiben zwei Punkte drin:** Das Relay-Log räumt nach 30 Tagen auf und
    nennt nur die ersten 8 Zeichen der Kennung. Die beiden anderen (Vor-Anmelde-Grenze je
    Adresse, TL-Zustand nur bei Zuschauern) brauchen eigene Entwürfe und gehen in die
    Roadmap.
19. **Die ferne Halle ist mit im Paket** (Frist, Backoff und Deckel der Sammelverbindung),
    ebenso die Begrenzung der Feldnummern, die ein Tablet anlegen kann.
20. **Ausdrücklich nicht gebaut:** Anmeldung des Turnier-PCs am Relay, Speicherung auf
    Platte, Cloudflare-Regeln, Sende-Fristen im Relay, Aufteilen des globalen Schlosses.

**Festlegungen dieser Spec, wo das How-To eine Lücke lässt**

21. **Die Monitor-Steuerung wird ohne verbundenen Turnier-PC abgewiesen** (409), wie der
    Bilder-Upload. Das How-To nennt die fehlende Prüfung als Befund, aber keine Regel.
22. **Der Namespace wird in PR 7 nach der Karenz entfernt, ab PR 8 nach 60 min.** Das
    How-To nennt an dieser Stelle noch die alte PR-Zählung.
23. **Die Close-Codes stehen in PR 2 als lokale Konstanten im Host** und wandern mit PR 7
    nach `relay-proto`; PR 9 stellt den Host darauf um. So bleibt PR 2 ohne Relay-Deploy.
24. **`/health` nennt ab PR 7 die Zahl verbundener Hosts** — das How-To setzt den Zähler
    als Prüfpunkt voraus, ohne ihn einem PR zuzuordnen.

## Akzeptanzkriterien

In Klammern der PR, mit dem das Kriterium erfüllt ist.

**Slots und Schlüssel**

- [ ] **AK1** (PR 3) Sendet eine Sitzung `identify` für Feld A und danach für Feld B,
  bleibt sie an A; B wird nicht belegt; es entsteht genau eine Log-Zeile je Sitzung. Nach
  dem Trennen hängt kein Eintrag dieser Sitzung mehr in `tablets`, und ein Namespace, an
  dem sonst nichts hängt, wird entfernt (ab PR 7 durch den Sweeper nach Ablauf seiner
  Frist).
- [ ] **AK2** (PR 3) Sendet der Halter eines Felds `identify` für dasselbe Feld erneut,
  bekommt er kein `session_superseded`, und der Host bekommt kein zweites
  `TabletConnected`.
- [ ] **AK3** (PR 3) Meldet ein Feld „belegt" und wird dann frei, greift das nächste
  `identify` derselben Sitzung.
- [ ] **AK4** (PR 3) Übernimmt B ein Feld von A und A übernimmt zurück, werden As Punkte
  wieder angenommen und Bs verworfen.
- [ ] **AK5** (PR 3) Ein Akkustand einer Sitzung, die das Feld nicht hält, erreicht den
  Host nicht.
- [ ] **AK6** (PR 3) Ein Tablet ohne Host hinterlässt nach dem Trennen keine Einträge für
  sein Feld. Ein Feld, das der Host kennt, behält Stand und Sätze für das Ersatzgerät.
- [ ] **AK7** (PR 3) Das 129. verschiedene Feld eines Namespace wird verworfen und einmal
  gemeldet; die Verbindung bleibt. Der Deckel gilt auch ohne Feldliste des Hosts.
- [ ] **AK8** (PR 3) Ein `take_over` legt keinen Namespace an, wenn `MAX_NAMESPACES`
  erreicht ist.

**Deckel und Kanäle**

- [ ] **AK9** (PR 4) Ein Score mit 11 Sätzen wird verworfen und einmal gemeldet, die
  Verbindung bleibt; 10 Sätze gehen durch.
- [ ] **AK10** (PR 4) Ein Tablet-Zustand mit 64 KiB + 1 Byte wird verworfen und gemeldet,
  die Verbindung bleibt. Eine Tablet-Nachricht über 1 MiB trennt die Verbindung.
- [ ] **AK11** (PR 4) Ein Träger-Umschlag mit 1,5 MiB trennt nicht; seine Nutzlast über
  160 KiB wird verworfen und einmal je Substrom gemeldet.
- [ ] **AK12** (PR 4) Ein Host-Frame mit dem größten fachlich zulässigen Zettel trennt
  nicht. Der Transport-Deckel liegt per Compile-Zeit-Zusicherung über jedem fachlichen
  Deckel.
- [ ] **AK13** (PR 4) Host-Frames über dem fachlichen Deckel trennen nie: Ein Brief über
  16 KiB wird verworfen und einmal gemeldet; ein Label über 128 Zeichen wird gekürzt; eine
  `Prepared`-Liste über 256 KiB wird zur leeren Liste; ein `TlAuth` mit einem Feld über 256
  Zeichen wird verworfen.
- [ ] **AK14** (PR 4) 60 Tablet-Frames auf einmal gehen durch; Dauerfeuer über 20 je
  Sekunde wird verworfen und einmal gemeldet; `ping` zählt nicht; ein Voll-Sync nach Abriss
  passt in den Takt.
- [ ] **AK15** (PR 4) `POST /{ns}/monitor/control` mit mehr als 256 Einträgen in einer
  Karte antwortet 413; ein Turniername über 256 Zeichen im Upload wird gekürzt.
- [ ] **AK16** (PR 5) Ist ein Empfänger-Kanal voll, wird genau dieser Empfänger getrennt,
  mit einer Log-Zeile; andere Verbindungen bleiben. Nach der Neuanmeldung hat das Tablet
  seine Zuweisung.

**Host**

- [ ] **AK17** (PR 1) Hängt ein Ergebnis-Write 20 s, beantwortet der Host ein Ping binnen
  1 s und bleibt verbunden; der Idle-Wächter beendet die Sitzung nicht.
- [ ] **AK18** (PR 1) Zwei Ergebnisse für dasselbe Feld werden nacheinander geschrieben,
  für verschiedene Felder nebeneinander. Der Retry eines hängenden Ergebnisses wird nach
  dessen Abschluss mit „ok" quittiert, ohne zweiten BTP-Write.
- [ ] **AK19** (PR 1) Reißt die Verbindung während eines Writes ab, läuft er zu Ende; über
  die neue Verbindung geht keine Quittung aus der alten Sitzung.
- [ ] **AK20** (PR 1) Das fünfte wartende Ergebnis eines Felds bekommt sofort eine
  wiederholbare Absage (nie „ok", nie „permanent").
- [ ] **AK21** (PR 1) Ein Werbe-Upload von 60 s blockiert weder Punkte noch Pings; ein
  zweiter startet nicht parallel; nach einem Fehlschlag wächst die Pause (30 s bis 5 min).
- [ ] **AK22** (PR 2) Ein Verbindungsversuch gegen einen stummen Port endet nach 10 s — im
  Host-Client und im Träger.
- [ ] **AK23** (PR 2) Nach einer Sitzung unter 30 s wächst die Pause; nach einer stabilen
  Sitzung beginnt sie bei 1 s. Bei Abweisung und bei einem Close ohne Grund bleibt sie bei
  höchstens 5 s; der Jitter bleibt im Band ± 20 %.
- [ ] **AK24** (PR 2) Der Träger sendet keine Frames, solange er nicht bereit ist, und
  keine Nutzlast über 160 KiB (einmal gemeldet).
- [ ] **AK25** (PR 9) Hält die Abweisung länger als 30 s an, zeigt das Dashboard „Ein
  anderer PC ist mit diesem Turnier verbunden"; nach erfolgreicher Verbindung verschwindet
  die Meldung. Mit altem Relay erscheint sie nie.
- [ ] **AK26** (PR 9) Der Host sendet nach jedem Verbinden ein `Courts`-Frame, auch ein
  leeres, und wiederholt den Werbe-Upload nach 409 binnen 3 s.

**Anzeigen**

- [ ] **AK27** (PR 7) Nach Host-Abgang liefern die fünf Stand-Routen (`court/{id}/state`,
  `monitor/state` eines zugewiesenen Geräts, `/{ns}/health`, `info/preparation/state`,
  `meetingpoint/state`) 20 s lang 200 mit dem letzten Stand, danach 503 mit `no-store`,
  `Retry-After: 2` und dem Rumpf `{"grund":"host_weg"}` — auch mit `If-None-Match` und bei
  warmem Cache.
- [ ] **AK28** (PR 7) Kommt der Host binnen 20 s zurück, hat keine Anzeige je 503 gesehen.
- [ ] **AK29** (PR 7) Ein stumm gestorbener Host ist spätestens 20 s nach seinem letzten
  Lebenszeichen gesperrt.
- [ ] **AK30** (PR 7) Ein Namespace ohne Tablets überlebt den Host-Abgang für die Karenz
  und liefert in dieser Zeit den letzten Stand.
- [ ] **AK31** (PR 7) Für einen Namespace, den es nie gab, antworten die Stand-Routen 503.
  Ein Gerät ohne Zuweisung bekommt 200 mit der Kopplungsseite und `hostOffline`.
- [ ] **AK32** (PR 7) In den ersten 20 s nach Relay-Start bekommt ein Abruf mit Marke für
  einen fehlenden Namespace 304, ohne Marke 503; danach 503.
- [ ] **AK33** (PR 7) Am Karenz-Ende und bei der Rückkehr danach werden alle Monitor-Abos
  genau einmal angestoßen; beim Host-Abgang werden die TL-Abos geschlossen.
- [ ] **AK34** (PR 7) `/courts` und `/info/ad/state` antworten ohne Host weiter 200;
  `/info/announce/state` antwortet ohne Host sofort 503.
- [ ] **AK35** (PR 7) `monitor.html` und `tafel.html` zeigen bei `host_weg` „Keine
  Verbindung zum Turnier-PC", dunkeln den Stand ab und laden kein Log hoch; `overview.html`,
  `combo.html` und `preparation.html` zeigen ihr Banner schon beim ersten solchen 503. Bei
  jedem anderen Fehler und im LAN verhalten sich alle fünf Seiten wie bisher.
- [ ] **AK36** (PR 7) Ein zweiter Host bekommt einen Close mit Code 4409 und `host_belegt`,
  am Namespace-Limit mit 4429 und `voll`. `/health` nennt die Zahl verbundener Hosts.

**Verfall und Schutz**

- [ ] **AK37** (PR 8) Der Sprachdienst-Schlüssel ist mit dem Host-Abgang aus dem Speicher.
- [ ] **AK38** (PR 8) Nach 60 min ohne Host sind Namen, Bilder, Logo, Listen und Freitext
  weg. Ein Feld mit verbundenem Tablet behält Stand und Sätze; das Tablet kann
  weiterzählen.
- [ ] **AK39** (PR 8) Kommt der Host nach dem Leeren zurück, wird der Stand eines Felds mit
  Tablet nicht geräumt (kein 0:0).
- [ ] **AK40** (PR 8) Kommt der Host vor Ablauf der 60 min zurück, ist nichts geleert.
- [ ] **AK41** (PR 8) Geht nach dem Leeren das letzte Tablet, verschwindet der Namespace.
  Ist die Tabelle voll, verdrängt ein neuer Host den am längsten hostlosen Namespace ohne
  Tablet; gibt es keinen, wird er abgewiesen.
- [ ] **AK42** (PR 6) Ein Bilder-Upload ohne verbundenen Host wird mit 409 abgewiesen, ohne
  den Rumpf zu lesen. Vor dem ersten `Courts`-Frame der Sitzung werden Bilder verworfen
  (409), die Konfiguration gilt; ein leeres `Courts`-Frame zählt. `monitor/control` ohne
  verbundenen Host antwortet 409.
- [ ] **AK43** (PR 6) Über dem Gesamt-Budget von 256 MiB antwortet der Upload 507 mit einer
  Log-Zeile, die Konfiguration gilt; das Budget sinkt beim Ersetzen und Entfernen von
  Bildern.
- [ ] **AK44** (PR 6) Das Relay-Log nennt nur die ersten 8 Zeichen der Kennung (im
  Quelltext steht kein `'{ns}'` mehr); es bleiben höchstens 30 Log-Dateien.
- [ ] **AK45** (PR 6) `ops/bts-relay.service` trägt `MemoryHigh`, `MemoryMax` und
  `MemorySwapMax`; `ops/nginx-bts-relay.conf` trägt das Body-Limit 25m und keine Limits;
  `docs/cloud-relay.md` beschreibt den Handgriff für die Dienst-Datei.

## PR-Zerlegung und Tests

Je PR: ein Patch-Bump in `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` und
`package.json` (die Nummer wird erst beim Merge festgelegt), Eintrag in
`docs/changelog.md`, `code-reviewer`. TDD — bei den neuen Befunden zuerst der rote Test.
Grün sein müssen `cargo test` (Workspace),
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`,
`npm run build`, `node scripts/test-handbuch.mjs` und
`node scripts/test-handbuch-fakten.mjs`. Zeit-Tests laufen mit
`#[tokio::test(start_paused = true)]`. Keiner der PRs ändert ein gespeichertes Format.

| PR | Inhalt | Relay-Deploy | `relay-proto` | `security-reviewer` |
|---|---|---|---|---|
| 1 | Host-Schleife entkoppeln | nein | nein | nein |
| 2 | Verbindungsaufbau, Träger | nein | nein | nein |
| 3 | Tote Slots, Halterschaft, Feld-Schlüssel | ja | nein | ja |
| 4 | Fachliche und Transport-Deckel, Mengen-Takt | ja | nein | ja |
| 5 | Begrenzte Kanäle | ja | nein | ja |
| 6 | Bilder-Tor, Gesamt-Budget, Log, Dienst-Datei, nginx-Vorlage | ja | nein | ja |
| 7 | Host-Verlust auf den Anzeigen, Close-Grund | ja | ja | ja |
| 8 | Daten-Verfall | ja | nein | ja |
| 9 | Dashboard-Meldung, `Courts` immer, Wiederholung bei 409 | nein | nein | nein |

### PR 1 — Host-Schleife entkoppeln

**Erfüllt:** AK17–AK21. **Lösung:** c).
**Dateien:** `src-tauri/src/tablet/relay_client.rs`.
**Tests:** `ping_wird_beantwortet_waehrend_ein_ergebnis_haengt` ·
`ergebnisse_eines_felds_laufen_nacheinander` · `felder_laufen_nebeneinander` ·
`retry_hinter_dem_ersten_versuch_wird_quittiert` ·
`quittung_geht_nie_in_eine_neue_sitzung` · `volle_feld_schlange_sagt_wiederholbar_ab` ·
`zweiter_upload_startet_nicht_solange_einer_laeuft` ·
`fehlgeschlagener_upload_wartet_laenger` ·
`sitzung_ueberlebt_pausen_mit_lebenszeichen_im_puffer` (ersetzt den geplanten
`idle_waechter_schlaegt_nach_langem_ergebnis_nicht_an`; hält nur fest, dass die Sitzung
16-s-Pausen mit Lebenszeichen im Puffer übersteht — der Wettlauf selbst lässt sich mit
angehaltener Uhr nicht nachstellen, der Test wurde deshalb nie rot; die feste Reihenfolge
im `select!` ist per Überlegung abgesichert) ·
`groesster_zettel_frame_bleibt_unter_dem_transportdeckel` (Messung für PR 4) ·
aus dem Review: `toter_arbeiter_wird_ersetzt` · `hintergrund_aufgaben_enden_mit_den_diensten` ·
`neue_sitzung_vergisst_die_upload_pause` · `haengender_schreiber_beendet_sich_nach_der_frist` ·
`unbekannte_felder_teilen_sich_einen_arbeiter`. AK21 (Upload blockiert weder Punkte noch
Pings) ist nur über die Wächter-Tests und den Aufbau belegt, nicht mit einem langsamen Upload.
**Doku:** `docs/cloud-relay.md` (Host-Schleife, Ergebnis-Reihenfolge), `docs/roadmap.md`,
`docs/changelog.md`; neue Zeile „Relay härten" in der Doku-Tabelle von `CLAUDE.md`
(angelegt hier, ergänzt in PR 2–9).

### PR 2 — Verbindungsaufbau, Träger

**Erfüllt:** AK22–AK24. **Lösung:** d).
**Dateien:** `relay_client.rs`, `carrier.rs`, neu `tablet/ws_verbindung.rs`.
**Tests:** `verbindung_gegen_stummen_port_endet_nach_frist` ·
`backoff_setzt_erst_nach_stabiler_sitzung_zurueck` ·
`abweisung_pausiert_hoechstens_fuenf_sekunden` · `close_ohne_grund_zaehlt_wie_abweisung` ·
`jitter_bleibt_im_band` · `traeger_verwirft_frames_ohne_bereitschaft` ·
`traeger_sendet_keine_nutzlast_ueber_dem_deckel`.
**Doku:** `docs/cloud-relay.md` (Frist, Backoff-Tabelle), `docs/multi-hall.md` (Träger),
`docs/roadmap.md`, `docs/changelog.md`.
Ein gemeinsamer Release-Tag für PR 1 und 2 ist möglich.

### PR 3 — Tote Slots, Halterschaft, Feld-Schlüssel

**Erfüllt:** AK1–AK8. **Lösung:** a).
**Dateien:** `relay/src/main.rs`.
**Tests:** `zweites_identify_fuer_anderes_feld_wird_verworfen` ·
`identify_des_halters_ist_folgenlos` (zuerst rot) · `belegt_retry_greift_nach_freigabe` ·
`uebernahme_hin_und_zurueck` (zuerst rot) · `akku_nur_vom_halter` ·
`host_unbekanntes_feld_wird_beim_abgang_geraeumt` ·
`host_bekanntes_feld_behaelt_den_stand` · `feld_schluessel_deckel_gilt_ohne_feldliste` ·
`namespace_ist_nach_feldwechsel_und_trennen_weg` ·
`uebernahme_legt_keinen_namespace_ueber_dem_limit_an`.
**Doku:** `docs/cloud-relay.md` (Deckel-Tabelle, Sicherheit), `docs/changelog.md`.

**Aus der Umsetzung (v0.9.384):** Die beiden als „zuerst rot" geplanten Tests
(`identify_des_halters_ist_folgenlos`, `uebernahme_hin_und_zurueck`) wurden nicht gegen den
alten Code laufen gelassen — sie nutzen den neuen Helfer `haelt_feld` und kompilieren dort
nicht. Die Fehler sind am alten Code nur gelesen, nicht nachgestellt. Der Feld-Deckel zählt
auch `court_hall`, `court_on_court_since` und die Monitor-Folge; `detach_tablet` räumt für
host-unbekannte Felder zusätzlich `court_hall` und `court_on_court_since`. Aus den Reviews:
eine abgewiesene Übernahme schließt die Verbindung; eine neue Feldliste räumt alte Feldnamen
und Hallen (sonst zählten sie auf den Deckel); `court_hall` zählt zu „host-bekannt". Offen
bleibt (späterer PR): eine Log-Zeile je Verbindung bei `identify` für ein fremdes Feld.

### PR 4 — Deckel und Mengen-Takt

**Erfüllt:** AK9–AK15. **Lösung:** b) ohne Kanäle.
**Dateien:** `relay/src/main.rs` (`router(broker, tl_an)` aus `main` herausgelöst),
`relay/Cargo.toml` (dev-dependency `tokio-tungstenite` 0.29).
**Vor dem Festlegen der Werte:** Ergebnis der Zettel-Messung aus PR 1; ein echter Brief
gemessen.
**Tests:** `elf_saetze_werden_verworfen_verbindung_bleibt` ·
`zu_grosser_brief_wird_einmal_gemeldet` · `lange_labels_werden_gekuerzt` ·
`prepared_ueber_deckel_leert_die_liste` · `eimer_laesst_schwung_durch_und_bremst_dann` ·
`voll_sync_nach_abriss_passt_in_den_takt` · `transport_deckel_liegen_ueber_fachlichen` ·
mit echtem WebSocket: `nachricht_ueber_transportdeckel_trennt`,
`zustand_ueber_64k_trennt_nicht`.
**Doku:** `docs/cloud-relay.md` (Deckel-Tabelle, Sicherheit), `docs/multi-hall.md`
(Träger-Umschlag), `docs/changelog.md`.

**Aus der Umsetzung (v0.9.385) — Abweichungen:**

- Takt der kompletten Abgleiche: **je ein eigener Eimer** für `rally_sync` und
  `match_event_sync`, Schwung **30, 2 je Sekunde** (statt gemeinsam 4 / 0,5). Jedes „zurück"
  schickt beide ohne Punkt-Sperre, es gibt 50 Schritte, und ein verworfener `rally_sync`
  wird nie wiederholt (Befund des Code-Reviews; ein erster Versuch mit Schwung 8 riss ab dem
  fünften schnellen „zurück").
- `state_sync` zählt **doch** im Takt (eigener Eimer, 30 / 3 je Sekunde): Sonst ließe sich
  mit 64-KiB-Zuständen im Dauerfeuer das eine Schloss aller Turniere belegen
  (Sicherheits-Review).
- `TlAuth`: Ein Eintrag mit überlangem Zugang wird **ausgelassen**, Kennung und Profil
  gekürzt — die übrige Liste gilt. „Frame verwerfen" hätte die alte Liste gültig gelassen und
  einen Widerruf im selben Push verschluckt.
- Offen (späterer PR): Werte der Monitor-Steuerung (`hall`, `file`, `court_ids`) und die
  Texte der Monitor-Konfiguration sind nur über das Body-Limit begrenzt; das Parsen des
  Zustands geschieht weiter unter dem Schloss.
- Nicht gebaut: das zusätzliche „einmal melden" für `rally_sync`/`match_event_sync` über
  ihren bestehenden Größendeckeln und für die Träger-Nutzlast über 160 KiB (beide werden wie
  bisher still verworfen); `Close(1009)` beim Überschreiten eines Transport-Deckels (die
  Verbindung endet ohne Close-Code); der Deckel für die Monitor-Konfiguration im Upload
  (nur der Turniername wird gekürzt).
- Die fachlichen Deckel der Host-Frames sitzen in einer reinen Funktion
  (`host_frame_deckeln`) vor den Armen, nicht in den Armen selbst.
- Messung: Ein echter Brief (Doppel, lange Namen, SR/AR) bleibt unter 2 KiB
  (`echter_brief_bleibt_weit_unter_dem_deckel`); der Zettel-Frame wurde in PR 1 gemessen.
- Statt `tokio-tungstenite` 0.29 als dev-dependency die 0.30 (liegt über die App ohnehin im
  Baum), dazu `futures-util`.

### PR 5 — Begrenzte Kanäle

**Erfüllt:** AK16. **Lösung:** b), Kanäle.
**Dateien:** `relay/src/main.rs`.
**Tests:** `voller_kanal_trennt_den_empfaenger` (echter WebSocket) ·
`ueberlauf_wird_einmal_gemeldet` · `tablet_bekommt_zuweisung_nach_neuanmeldung` ·
`same_channel_unterscheidet_weiter`.
**Doku:** `docs/cloud-relay.md` (Überlast, Fehlersuche: Log-Zeile „Kanal voll"),
`docs/changelog.md`.
**Prüfpunkt nach dem Deploy:** Die Log-Zeile „Kanal voll" darf im Normalbetrieb nicht
auftreten.

**Aus der Umsetzung (v0.9.386):** `Tx` ist ein kleiner Typ um `mpsc::Sender` mit einem
geteilten Überlauf-Merkmal; die Wache (`UeberlaufWache`) ist ein eigener Griff neben dem
Empfänger (sonst kollidierten `recv` und `warten` im selben `select!`). Der Meldekanal
„Substrom tot" des Trägers (`tot_tx`, trägt nur Kennungen) bleibt unbegrenzt. „Einmal
melden" ergibt sich daraus, dass die Schleife nach der Meldung endet. Der Test mit echtem
WebSocket füllt den Kanal aus dem Test heraus (am Empfänger vorbei), weil sich ein echter
Stau über die Puffer des Betriebssystems nicht zuverlässig herstellen lässt.

Aus den Reviews:
- **Reserve am Kanal des Turnier-PCs** (`Tx::send_nachrangig`): Tablet-Frames belegen
  höchstens drei Viertel und lösen nie die Trennung aus. Sonst hätte dieser PR einen neuen
  Hebel geschaffen — mit der Turnier-Kennung den Turnier-PC trennen lassen und seinen Platz
  besetzen (Sicherheits-Review, bedingt hoch). Das weicht von „nichts wird still verworfen"
  ab: Über der Marke werden Tablet-Frames verworfen (einmal gemeldet); sie heilen über den
  nächsten Frame bzw. Abgleich.
- `POST /{ns}/result`: Body 16 KiB, höchstens zehn Sätze, Feldname gekürzt.
- „Getrennt" gilt, sobald die Schleife wieder läuft; eine Schreibfrist am Relay bleibt
  Nicht-Ziel. Die Angabe „100 s Spitzenverkehr" war nicht belegt und ist ersetzt.
- Offen: Anstöße an Monitore je Feld zusammenfassen; Takt für `identify`/`take_over`;
  Close-Frame beim Trennen; Tests mit echtem WebSocket für Turnier-PC, Monitor und Träger.

### PR 6 — Bilder-Tor, Gesamt-Budget, Log, Dienst-Datei, nginx-Vorlage

**Erfüllt:** AK42–AK45. **Lösung:** g), h).
**Dateien:** `relay/src/main.rs`, `ops/bts-relay.service`, `ops/nginx-bts-relay.conf`.
**Tests:** `upload_ohne_host_wird_vor_dem_rumpf_abgewiesen` (zuerst rot) ·
`bilder_erst_nach_feldliste_konfiguration_sofort` · `leere_feldliste_zaehlt` ·
`gesamt_budget_weist_bilder_ab` · `budget_sinkt_beim_ersetzen_und_entfernen` ·
`kennung_im_log_hat_acht_zeichen` · Wächter: keine Stelle mit `'{ns}'` im Quelltext.
**Doku:** `docs/cloud-relay.md` (Sicherheit; Deployment mit dem Handgriff für die
Dienst-Datei), `docs/logging.md` (Verfall, gekürzte Kennung, Such-Rezept),
`docs/changelog.md`.
**Handgriff:** Dienst-Datei am Server, im selben Fenster (siehe „Deploy-Regeln").

### PR 7 — Host-Verlust auf den Anzeigen, Close-Grund

**Erfüllt:** AK27–AK36. **Lösung:** e), Close-Grund aus d).
**Dateien:** `relay/src/main.rs`, `relay-proto/src/lib.rs`,
`src-tauri/assets/{monitor,tafel,overview,combo,preparation}.html`.
**Tests:** `anzeige_lage_grenzen` · `host_weg_zeigt_karenz_lang_200_dann_503` (jede Route) ·
`gesperrt_schlaegt_cache_und_304` · `stummer_host_zaehlt_ab_letztem_lebenszeichen` ·
`namespace_ueberlebt_host_abgang_ohne_tablets` · `fehlender_namespace_ist_gesperrt` ·
`start_karenz_bestaetigt_mit_marke` · `unzugewiesenes_geraet_behaelt_kopplungsseite` ·
`ansage_route_sperrt_sofort` · `feldliste_bleibt_abrufbar` ·
`karenz_ende_stoesst_alle_abos_an` · `rueckkehr_stoesst_an` ·
`tl_abos_werden_beim_abgang_geschlossen` · `zweiter_host_bekommt_close_grund` (echter
WebSocket). Die Inline-Kopien der Seiten bleiben unter `scripts/test-inline-kopien.mjs`
grün.
**Doku:** `docs/cloud-relay.md` (Close-Grund; die falsche Aussage zur „Zweiter
Host"-Warnung, `:1136–1137`, berichtigen; Routen-Tabelle), `docs/court-monitor.md` und
`docs/oberflaechen.md` mit `pruef:`-Anker für „Keine Verbindung zum Turnier-PC" in
`monitor.html` und `tafel.html`, `docs/multi-hall.md` (ferne Halle, LAN zählt weiter),
`docs/changelog.md`.

### PR 8 — Daten-Verfall

**Erfüllt:** AK37–AK41. **Lösung:** f).
**Dateien:** `relay/src/main.rs`.
**Tests:** `sprachschluessel_ist_sofort_weg` · `nach_sechzig_minuten_sind_turnierdaten_weg` ·
`feld_mit_tablet_behaelt_stand_und_stummel` · `tablet_zaehlt_nach_dem_leeren_weiter` ·
`rueckkehr_nach_leeren_raeumt_stand_nicht` ·
`namespace_verschwindet_wenn_letztes_tablet_geht` ·
`voller_relay_verdraengt_den_aeltesten_hostlosen` ·
`rueckkehr_vor_ablauf_bricht_verfall_ab`.
**Doku:** `docs/cloud-relay.md` (Lebensdauer, Datenschutz im Abschnitt „Sicherheit"),
`docs/changelog.md`.

### PR 9 — Dashboard-Meldung, `Courts` immer, Wiederholung bei 409

**Erfüllt:** AK25, AK26. **Lösung:** Dashboard aus d), neuer Host aus g).
**Dateien:** `relay_client.rs`, `tablet/state.rs`, `commands.rs`,
`src/components/AlertBanner.tsx`, `src/types.ts`.
**Voraussetzung:** PR 7 ist ausgerollt, und der Close-Code kommt durch nginx und Cloudflare
beim Host an (Annahme 1).
**Tests:** `abweisung_wird_erst_nach_dreissig_sekunden_gemeldet` ·
`meldung_verschwindet_nach_verbindung` · Handbuch-Anker.
**Doku:** `docs/oberflaechen.md` mit `pruef:`-Anker für „Ein anderer PC ist mit diesem
Turnier verbunden", `docs/cloud-relay.md`, `docs/changelog.md`, `docs/roadmap.md`
(Abschluss des Pakets).

### Reihenfolge

1. **PR 1 zuerst:** stärkster Code-Beleg (Schreib-Gate, 15-s-Frist, falscher Idle), kein
   Deploy, und jeder vermiedene Host-Abwurf schließt das Fenster zur Übernahme des Slots.
2. **PR 2**, ebenfalls ohne Deploy.
3. **PR 3, 4, 6** im ersten Relay-Fenster.
4. **PR 5, 7, 8** im zweiten Fenster.
5. **PR 9** nach dem Deploy von PR 7.

## Risiken & Rollback

**Risiken im laufenden Turnier**

- **Jeder Relay-Merge leert den Speicher und trennt alle Verbindungen** — sechs Mal in
  diesem Paket, dazu einmal der Handgriff für die Dienst-Datei. Absicherung: zwei Fenster,
  `/health`-Prüfung, Start-Karenz ab PR 7.
- **PR 5 ist der riskanteste Umbau.** Ein zu kleiner Kanal trennt gesunde Geräte. Prüfpunkt
  ist die Log-Zeile „Kanal voll" nach dem Deploy.
- **PR 7 ändert, was Zuschauer sehen.** Ein zu kurzer Wert ließe Fernseher bei
  Host-Wacklern blinken; die 20 s stehen als eine Konstante.
- **PR 4:** Schlüssel-Deckel (128) und Brief-Deckel (16 KiB) sind geschätzt. Ein zu knapper
  Brief-Deckel ließe ein Tablet ohne Zuweisung; deshalb die Messung vor dem Festlegen und
  die Meldung im Log.
- **PR 6:** Die Speicher-Obergrenze beendet den Dienst, wenn sie zu knapp ist — für alle
  Turniere. Deshalb erst den Ist-Verbrauch ablesen.
- **PR 6, alter Host:** nach einem Relay-Neustart 30 s ohne Werbung.
- **PR 1:** Ein Fehler im Arbeiter je Feld träfe den Ergebnis-Weg. Absicherung:
  `process_result` bleibt unberührt, die Idempotenz aus ADR 0018 fängt Doppelungen, das
  Tablet puffert und wiederholt.

**Rollback.** Jeder PR ist einzeln per Revert zurückzunehmen; kein Config-Feld, keine Datei
auf der Platte, ein neues Wire-Detail (Close-Code), das Ältere ignorieren. Reihenfolge beim
Zurücknehmen: 9 → 8 → 7 → 5 → 6 → 4 → 3 → 2 → 1. Einzelheiten unter „Deploy-Regeln".

## Bewusste Abwägungen

- **Kein Stand vom Relay für das Ersatzgerät eines host-unbekannten Felds.** Betrifft nur
  das Zählen ohne Turnier-PC nach einem Relay-Neustart; dafür wächst der Speicher nicht
  mehr unbegrenzt.
- **Start-Karenz bestätigt ohne eigenen Stand.** Höchstens 20 s lang antwortet der Relay
  „unverändert", ohne es zu wissen. Dafür bleibt ein Neustart unsichtbar.
- **Nach dem Fehlen des Namespace sehen zugewiesene Monitore die Kopplungsseite** mit
  Hinweis statt des abgedunkelten Stands. Der Relay kennt die Zuweisung nicht mehr.
- **`stream.html` zeigt nie einen Fehlertext.** In einer Stream-Einblendung ist eine
  durchsichtige Fläche besser als eine Störungsmeldung.
- **Trennen statt Verwerfen bei vollem Kanal.** Ein getrenntes Gerät erholt sich von
  selbst; eine still verlorene Zuweisung nicht.
- **Der Host-Slot bleibt übernehmbar.** Siehe „Sicherheit".
- **Die Dashboard-Meldung kommt mit 30 s Verzug** — sonst erschiene sie nach jedem
  Netzwechsel über die eigene tote Verbindung.
- **Der Wortlaut auf alten Seiten bleibt alt**, bis die ferne Halle die App aktualisiert.
- **Feld-Schlüssel und Halterschaft ohne eigenen ADR.**

## Offene Fragen / Annahmen

Annahmen des How-To, die nicht am Code belegt sind:

| Nr. | Annahme | Zu prüfen in |
|---|---|---|
| 1 | nginx und Cloudflare reichen Close-Frames mit Code 4409 unverändert durch | nach dem Deploy von PR 7, vor PR 9 (sonst erscheint die Dashboard-Meldung nie; der Backoff bleibt über die Zeile „Close ohne Grund" richtig) |
| 2 | Der Server läuft mit cgroup v2, `MemoryMax`/`MemorySwapMax` wirken wie beschrieben; der heutige Speicherverbrauch ist ungemessen | PR 6 (Handgriff, Schritt 1 und 4) |
| 3 | Der Dienst-Benutzer darf im Log-Verzeichnis alte Dateien löschen, auch solche des früheren Benutzers | PR 6 (nach dem Deploy am Verzeichnis prüfen) |
| 4 | Der JSON-Aufschlag eines Zettels liegt unter dem Doppelten; die Größe eines 40-Blatt-Dokuments ist ungemessen | PR 1 (Messtest), entscheidet den Host-Deckel in PR 4 |
| 5 | Der Anteil von Anführungszeichen in einem Format-1-Zustand (Rechnung zum Träger-Umschlag) ist geschätzt | PR 4 (Test mit einem alten Zustand) |
| 6 | badhub.de hat keine Route `/pi-log` (laut Kommentar in `combo.html`) | PR 7 |
| 7 | `ctx.http` ist der Client mit 15 s Gesamtfrist | PR 1 |
| 8 | Der gespiegelte Tablet-Zustand enthält keine Spielernamen | PR 8 — das Audit nennt das Gegenteil (der Brief liegt im `state_sync`). Trifft das zu, bleibt für Felder **mit verbundenem Tablet** nach den 60 min ein Rest an Namen im Speicher, bis das Tablet geht; das ist dann in `docs/cloud-relay.md` so zu benennen |
| 9 | Rück-Übernahme, Selbst-Verdrängung und falscher Idle sind aus dem Code abgeleitet, nicht nachgestellt | rote Tests in PR 3 (zwei) und PR 1 (einer) |
| 10 | Ein Brief mit Doppel, Schiedsrichtern und Zähltafelbediener bleibt unter 4 KiB | PR 4 (Messung vor dem Festlegen) |
| 11 | Die tungstenite-Standardgrenzen (64 MiB / 16 MiB) und das Fehlen eines automatischen Close 1009 gelten für die Version im Lockfile | PR 4 (echter WebSocket-Test) |

Offene Fragen an den Nutzer gibt es keine; alles Weitere steht unter „Zur Freigabe
ausdrücklich vorgelegt".

## Betroffene Doku-Dateien

- `docs/cloud-relay.md` — in jedem PR: Host-Schleife und Backoff (PR 1, 2), Deckel-Tabelle
  und Überlast (PR 3–5), Sicherheit und Deployment mit Handgriff (PR 6), Close-Grund,
  Routen bei Host-Verlust und Berichtigung der „Zweiter Host"-Aussage (PR 7), Lebensdauer
  (PR 8), Dashboard-Meldung (PR 9).
- `docs/court-monitor.md`, `docs/oberflaechen.md` — Verhalten und Wortlaut bei Host-Verlust
  mit `pruef:`-Ankern (PR 7); Dashboard-Meldung mit `pruef:`-Anker (PR 9).
- `docs/multi-hall.md` — Träger (PR 2, 4), ferne Halle und LAN+Cloud bei Host-Verlust
  (PR 7).
- `docs/logging.md` — Log-Verfall, gekürzte Kennung, Such-Rezept (PR 6).
- `docs/changelog.md` — je Version.
- `docs/roadmap.md` — Stand je PR; die ausgelagerten Punkte stehen dort schon.
- `CLAUDE.md` — neue Zeile „Relay härten" in der Doku-Tabelle (Code-Stellen dieser Spec →
  diese Spec, `docs/cloud-relay.md`, `docs/court-monitor.md`, ADR 0088–0091), angelegt in
  PR 1, ergänzt in PR 2–9.
- ADR [0088](../adr/0088-anzeigen-bei-host-verlust-karenz-dann-503.md),
  [0089](../adr/0089-lebensdauer-des-namespace-zustands-ohne-host.md),
  [0090](../adr/0090-ueberlast-am-relay-und-mindestschutz.md),
  [0091](../adr/0091-ergebnisse-im-host-ein-arbeiter-je-feld.md).

## Umsetzungs-Hinweise

- Erst nach der Freigabe dieser Spec wird Produktivcode geschrieben.
- Muster im Haus: `melde_state_ueber_deckel` (`relay/src/main.rs:590`; verwerfen, einmal
  melden), `host_weitergabe_erlaubt` (`:3944`; Eimer-Verfahren), der gespawnte
  `TlCommand`-Arm in `relay_client.rs`, `#[tokio::test(start_paused = true)]` in
  `btp/write_gate.rs`.
- Reine Bausteine zuerst, mit Tests: im Relay `Eimer::nimm`, `feld_ist_host_bekannt`,
  `feld_schluessel_erlaubt`, `anzeige_lage`, `lage_ohne_namespace`, `verfall_schritt`,
  `turnierdaten_leeren`, `brief_anonymisieren`, `bilder_tor`, `ns_kurz`; im Host
  `Backoff::naechste_pause`, `verbinde_mit_frist`, `ErgebnisArbeiter`, `sitzung_fahren`,
  `schliess_grund`.
- Ein echter WebSocket-Test ist nur für Transport-Deckel, Close-Code und „voller Kanal
  trennt" nötig; alles andere ruft Funktionen direkt auf, wie die vorhandenen Relay-Tests.
- Nichts gegen den Produktiv-Relay ausführen; kein Relay-Merge während eines Turniers.
- Kommentare im Code auf Deutsch (was und warum).
