# Diagnose-Logs

bts-light schreibt ein Log, mit dem sich Fehler nachvollziehen lassen –
lokal auf dem Rechner und optional zentral auf badhub.de.

## Lokale Logdatei

- bts-light schreibt eine **tägliche Logdatei** `bts-light.log` ins
  App-Log-Verzeichnis (`tracing` + Rolling-File-Appender).
- Im Dashboard öffnet der Button **„Logs öffnen"** das Verzeichnis im
  Datei-Manager.
- Bewusst **nicht** im Installationspfad: `Programme\…` ist für normale
  Nutzer nicht beschreibbar und wird bei Updates überschrieben.

Protokolliert werden u. a.:

- App-Start mit Versionsnummer
- Start des Tablet-Servers (mit LAN-Adresse)
- ausgelieferte Tablet-Seiten, Tablet verbunden/getrennt, Match-Zuweisung
- Ergebnis-Übermittlung vom Tablet und die BTP-Antwort (`SENDUPDATE`)
- **abgelehnte** Ergebnisse vom Tablet (seit v0.9.254) — mit Feld, Match und
  Grund. Vorher wurde nur der Erfolg protokolliert; ein Tablet, das sein Spiel
  nicht abschließen konnte, hinterließ auf dem Turnier-PC keine Spur, und die
  Ursache ließ sich nur am Gerät selbst finden (Feldtest 22.08.2026). Der
  Grund steht zusätzlich im Tablet-Log unter `submit_rejected`.
- **Störungen der badhub-Strecke und BTP-Fehler** (seit v0.9.381) —
  flankengesteuert: je Zustandswechsel eine Zeile (`badhub-Strecke: gestoert`,
  `… gedrosselt`, `… angehalten (Passwort)`, `… tset abgelehnt`,
  `BTP nicht erreichbar`), solange der Zustand anhält höchstens alle zehn
  Minuten eine Erinnerung mit der Zahl der Versuche, bei der Rückkehr eine
  Zeile mit der Dauer. Die Zeilen nennen Kanal, Klasse, HTTP-Status, Größe
  und Dauer — keinen Körper, keinen Namen, kein Passwort. Bis v0.9.380 stand
  ein gescheiterter `tset` und ein BTP-Fehler **gar nicht** im Log (nur in der
  Statuszeile), die additiven Kanäle dagegen in jedem 5-s-Zyklus. Details:
  [liveticker-push.md](liveticker-push.md).
- **Sammelzeile der badhub-Strecke** (seit v0.9.391) — solange gesendet
  wird, je Minute **eine** Zeile `badhub-Messung (60 s): …`. Sie nennt je
  Kanal die Zahl der Sendungen, Median und Maximum der Dauer, den größten
  Körper in Bytes und die Fehlschläge (keine Antwort oder kein 2xx), dazu
  `spielplan geaendert=… gleich=…`: wie oft der Spielplan gegenüber dem
  vorigen derselben Sitzung anders bzw. unverändert hinausging (der erste Spielplan nach dem Starten
  oder nach dem Speichern der Einstellungen zählt als geändert). Beispiel:
  `badhub-Messung (60 s): tupdate_match n=118 median=85ms max=420ms
  groesste=77B fehler=0; sched n=1 median=900ms max=900ms groesste=458256B
  fehler=0; spielplan geaendert=0 gleich=1`. Nur Kanalnamen und Zahlen. Die
  Zeile entsteht mit der ersten Sendung nach Ablauf der Minute; die letzte
  angefangene Minute vor dem Stoppen fehlt. Zweck: die Feldmessung der Spec
  `badhub-push-entkoppeln` — die Zeile `badhub-Sendung` je Sendung steht auf
  Debug-Stufe und erreicht das Log (Info-Stufe) nicht.
- **Perf-Zeile der Anzeige-Strecke** (seit v0.9.235, siehe unten)

### Stillstands-Wächter der Anzeigen (seit v0.9.255)

Court-Monitore froren gelegentlich auf einem alten Stand ein: Die Seite lief
weiter und rendert weiter, sie übernahm nur nichts Neues mehr. Dieses Symptom
hinterließ **keine Spur** — der Log-Upload hängt an JS-Fehler,
`unhandledrejection` und `pagehide`, und ein stiller Hänger fällt durch alle
drei Raster. Am Ende stand „hängt öfters mal" ohne jede Möglichkeit, die
Ursache einzugrenzen.

`overview.html` und `monitor.html` prüfen deshalb alle 10 Sekunden zwei
Größen: wann zuletzt ein Abruf **ohne Fehler** zurückkam und wann zuletzt ein
Stand **angewendet oder als aktuell bestätigt** wurde (ein `304` „nichts
Neues" zählt mit — sonst meldete eine ruhige Halle im Minutentakt
Fehlalarme). Bleibt eines davon länger als **60 Sekunden** aus, schreibt die
Seite eine `error`-Zeile `stillstand` und lädt ihr Log sofort hoch; erholt sie
sich, folgt `stillstand_vorbei`. Gemeldet wird **einmal je Episode**, nicht im
Takt.

Die Zeile trennt die beiden Welten, die am hängenden Gerät nicht
unterscheidbar waren:

| `art` | Bedeutung |
|---|---|
| `keine_abrufe` | Es kommt gar nichts mehr zurück — Poll-Timer tot, Netz oder Server weg. |
| `verworfen` | Die Abrufe klappen, aber nichts landet auf dem Schirm: Die Seite selbst verwirft. |

Mitgeschrieben werden außerdem `stillMs`, die Zahl der abgewiesenen Stände
bzw. Anstöße (`verworfen` / `verworfeneNudges`), der WS-Zustand, das Alter des
letzten Server-Frames, der aktuelle Fallback-Takt und — beim Court-Monitor —
Feld und gezeigte Ordnungszahl.

**Der Wächter heilt nichts.** Kein Reload, kein Zurücksetzen der
Ordnungszahl: Solange unklar ist, woran es liegt, würde eine Selbstheilung
genau die Spur verwischen, wegen der er gebaut wurde. Die Regel steht in
`src/io/standstill.mjs` (Test `scripts/test-standstill.mjs`, eigener
CI-Schritt); beide Anzeige-Seiten tragen eine Inline-Kopie.

Eine Ursache für `keine_abrufe` ist seit v0.9.287 beseitigt: Der Stand-Abruf
hatte keinen Timeout, und ein einzelner im WLAN-Roaming verlorener Abruf
blockierte alle folgenden (Turnier 05./06.09.2026, siehe
[court-monitor.md](court-monitor.md), „Frist für den Stand-Abruf"). Seither
endet er als Fehler, wenn die Kopfzeilen nicht binnen 5 s da sind oder der
Rumpf danach länger als 15 s braucht — im schlimmsten Fall also nach 20 s;
im Monitor-Log erscheint dann `offline` mit anschließendem `online` statt
eines minutenlangen `stillstand`.

**Fehlalarm bis v0.9.288 (Court-Monitor):** `monitor.html` zählte ein `304`
weder als Abruf noch als bestätigten Stand — nur ein voller `200`. Bei
gesundem Push-Kanal erzwingt der Refetch-Cap einen solchen aber nur alle
10 Minuten; 60 s danach meldete jeder Court-Monitor `keine_abrufe` mit
`fetchOk=true`, `wsOpen=true` und frischem Herzschlag (`frameAltMs` ≈ 10 s),
neun Minuten später `stillstand_vorbei`, eine Minute darauf wieder
`stillstand` — exakt im 10-Minuten-Raster, in jeder Spielpause. Beim Turnier
am 12./13.09.2026 waren das ~50 Meldungen je Gerät und Tag, jede mit
Log-Upload, und ein echter `keine_abrufe` wäre darin untergegangen. Seit
v0.9.291 verbucht auch der Court-Monitor das `304` wie die Feld-Übersicht
(`abrufGeglueckt()` + `letzterStandMs`); `scripts/test-standstill.mjs` prüft
das im Quelltext beider Seiten. Eine Ausnahme bleibt: Hat der `seq`-Guard den
letzten vollen Stand verworfen, trägt die Marke einen nie gezeigten Stand —
dann stempelt das 304 den Stand **nicht**, damit `verworfen` weiterhin
auffliegt. Die Zeile trägt seither wie bei der Übersicht die Zahl der
`bestaetigungen`. Ein `stillstand` mit
`art=keine_abrufe` und `fetchOk=true` ab v0.9.291 ist damit wieder ein
**echter** Befund — vorher ist er mit hoher Wahrscheinlichkeit dieser
Fehlalarm.

### Perf-Zeile der Anzeige-Strecke

Alle zehn Sekunden eine Zeile mit dem, was die Monitore und Übersichten im
vergangenen Fenster gekostet haben (Spec
[features/monitor-livestand-push.md](features/monitor-livestand-push.md),
Etappe S0):

```
Perf-Anzeigen (10 s): /health 40 push + 800 poll = 84.0/s, 1.63 MB/s ·
/court/state 12 push + 0 poll, 0.01 MB/s · overview 840 Bauten (84.0/s),
p95 4.19 ms / max 16.73 ms (seit Start) · live-scores 210 Schreibvorgänge
(21.0/s, 0.04 MB/s, Ø 20.00 ms) · 210 Nudges (21.0/s)
```

Drei Dinge dazu:

- **Sie steht im Sync-Takt, nicht im Server.** Der Sync-Loop läuft in
  beiden Betriebsarten, der LAN-Server nicht. Sie wird **vor** dem
  BTP-Abruf gezogen, damit sie auch dann kommt, wenn BTP klemmt: Die
  Anzeigen laufen ja weiter.
- **Im Cloud-Betrieb fehlt die Abruf-Seite.** Dort bedient der Relay
  `/health` und `/court/{id}/state`, nicht der Turnier-PC — die beiden
  Abruf-Zähler stehen dann auf null, während Nudges, Schreibvorgänge und
  Übersichts-Bauten normal weiterzählen. Eine Zeile ohne Abrufe heißt im
  Cloud-Modus also **nicht** „keine Anzeigen". Wer die Cloud-Abrufe
  messen will, fährt `scripts/last-monitor.mjs` gegen die Relay-Adresse;
  es zählt von außen mit.
- **Stille schreibt nichts.** Hängt keine Anzeige am PC, entfällt die
  Zeile ganz statt lauter Nullen zu protokollieren.
- **Nur Zahlen** — keine Namen, keine Match-IDs. Derselbe Stand steht im
  LAN unter `GET /debug/perf` als JSON zur Verfügung; ein Wächter-Test
  hält den Personenbezug draußen.

Der Zweck ist der Rückweg: In einem echten Turnier steht kein Messgerät,
aber das Log kommt über den Upload zurück.

### Suche nach der TP-Datei

Jede Suche nach der in BTP geöffneten Turnierdatei (Spec
[tp-datei-finden](features/tp-datei-finden.md)) schreibt eine Zeile
`TP-Datei-Suche: …` mit dem Ausgang — etwa `gefunden (Turnier.TP)`,
`BTP läuft nicht`, `kein Zugriff auf BTP` oder `Abfrage hängt`. Genannt wird
nur der **Dateiname**, nie der Pfad: Er enthält den Windows-Benutzernamen,
und die Logs werden hochgeladen.

Die Automatik außerhalb der Einstellungen (seit v0.9.424) schreibt nur, wenn
sich etwas tut — `TP-Datei-Automatik: gefunden (…), übernommen`,
`… Rückfrage gestellt (…)`, `… Rückfrage beantwortet (Ja|Nein), Neustart: …`
und als Warnung `… Abfrage hängt`. Der Minutentakt ohne Befund bleibt still.

### Seiten-Paket beim Start (seit v0.9.432)

Liegt im App-Datenverzeichnis unter `seiten/` ein nachgeladenes Seiten-Paket
(Spec [`hotfix-ohne-neustart`](features/hotfix-ohne-neustart.md)), prüft
bts-light es bei jedem Start vollständig neu. Die Zeilen nennen Folge und
Grund, nie Inhalte:

| Zeile | Bedeutung |
|---|---|
| `Seiten-Paket Folge N aktiv (Commit …)` | Das Paket hat alle Prüfungen bestanden; die Geräte-Seiten kommen aus ihm. |
| `Seiten-Paket auf der Platte abgelehnt (…) – beiseitegelegt, es gilt der eingebaute Stand` | Signatur, Format, Seitenmenge, Platzhalter, Version oder Folge passten nicht. Die Datei liegt danach als `abgelehnt.paket` daneben. |
| `Seiten-Paket unlesbar (…) – beiseitegelegt, …` | Die Datei hat nicht die erwartete Form oder ist zu groß. |
| `Seiten-Paket ohne lesbaren Zustand – beiseitegelegt, …` | `zustand.json` fehlt oder ist kaputt; damit ist unbekannt, welche Folgen zurückgenommen wurden. |
| `Seiten-Paket gerade nicht lesbar (…) – für diesen Lauf gilt der eingebaute Stand` | Das System hat das Lesen verweigert (etwa ein Virenscanner). Die Datei bleibt liegen und gilt beim nächsten Start wieder. |
| `Seiten-Paket ist keine gewöhnliche Datei – …` | Unter dem Namen des Pakets liegt etwas anderes; es wird nicht geöffnet. |
| `Seiten-Paket gehört zu einer anderen Version – verworfen, …` | Nach einem Update der App: Es gelten die Seiten der neuen Version. |
| `Seiten-Pakete aus: …` | Der Prüfschlüssel oder das App-Datenverzeichnis fehlt; es gilt der eingebaute Stand. |

Ohne Paket im Verzeichnis erscheint keine Zeile.

### Seiten-Paket: Angebot, Übernahme, Rückweg (seit v0.9.433)

| Zeile | Bedeutung |
|---|---|
| `Seiten-Paket Folge N geladen, wartet auf die Übernahme` | Im Hintergrund geladen und geprüft; das Banner bietet es an. Noch ist nichts gewechselt. |
| `Seiten-Paket abgelehnt, nichts geändert: …` | Ein geladenes Paket bestand die Prüfung nicht. Kein Banner; der Grund steht auch unter **Wartung**. Dieselbe Folge wird danach nicht noch einmal geladen. |
| `Seiten-Paket Folge N übernommen (vorher Folge M, Commit …)` | Klick auf „Jetzt übernehmen": Die Seiten sind gewechselt. |
| `Seiten-Paket nicht übernommen: …` | Das Ablegen scheiterte (etwa Platte voll); der aktive Stand ist unverändert. |
| `Seiten-Paket Folge N zurückgenommen und gesperrt, aktiv ist Folge M` | Klick auf „Zurück zum vorigen Stand". |
| `Voriges Seiten-Paket abgelehnt (…) – zurück zum eingebauten Stand` | Beim Rückweg bestand das vorige Paket die Prüfung nicht mehr. |
| `Voriges Seiten-Paket ließ sich nicht aktivieren (…) – zurück zum eingebauten Stand` | Beim Rückweg scheiterte das Umbenennen; es gelten die eingebauten Seiten. |
| `Seiten-Rückweg gescheitert: …` | Schon die Sperre ließ sich nicht schreiben; nichts wurde geändert. |

Ein Netzfehler beim Suchen oder Laden erzeugt bewusst **keine** Zeile —
offline ist in der Halle der Normalfall.

### Wechsel ohne Installer (seit v0.9.439)

| Zeile | Bedeutung |
|---|---|
| `Kernwechsel auf vX vorbereitet (Probelauf bestanden, Rückweg möglich/nicht möglich)` | Die neue Programmdatei liegt geprüft daneben; das Banner bietet „Jetzt wechseln" an. |
| `Kernwechsel ohne Installer nicht möglich: …` | Kein Angebot; es bleibt beim Installations-Weg. Dauerhafte Gründe (kein neuer Versuch): nur unter Windows, kein Schreibrecht, Kopf oder Programmdatei abgelehnt, Probelauf gescheitert. Vorübergehende (bis zu vier Versuche, je Prüflauf einer): kein Kern-Artefakt veröffentlicht, Netz weg. |
| `Kernwechsel auf vX: Programmdatei getauscht` | Schritt 1 des Wechsels — die Übertragung läuft noch. |
| `Kernwechsel auf vX: Wächter gestartet (Prozess N), App endet` | Die Übertragung ist angehalten; ab hier zählt die Lücke. |
| `Kernwechsel: Ankunft von vX gemeldet` | Die neue Version läuft und hat sich beim Wächter gemeldet. |
| `Kernwechsel abgebrochen: …` | Es wurde nicht gewechselt. Die Programmdatei ist unverändert oder wurde zurückgelegt; ließ sie sich nicht zurücklegen, steht das im Text. |
| `Rückweg auf vY: Programmdatei getauscht` | Die Vorversion liegt wieder an ihrem Platz, die laufende ist die neue Vorversion. Danach folgen dieselben Zeilen wie beim Wechsel (Wächter, Ankunft). |
| `Rückweg abgebrochen: …` | Es wurde nicht zurückgewechselt; im Text steht, ob etwas zurückgelegt werden musste. |
| `Vorversion vX für den Rückweg gesichert` | Vor einem Update über das Installationsprogramm wurde die laufende Programmdatei daneben gelegt. |
| `Vorversion nicht gesichert – nach dem Update kein Rückweg: …` | Das Update läuft trotzdem, nur ohne Rückweg. |
| `Update vX wurde zurückgenommen und wird nicht erneut angeboten` | Die Update-Prüfung fand eine Version, von der hier zurückgewechselt wurde. |
| `Aufräumen beim Start: …` | Je entfernter Datei eine Zeile (Rest eines Vorladens, nicht angelaufene Programmdatei, veraltete Vorversion, alter Installer) — oder warum sie sich nicht entfernen ließ. |

Der Wächter selbst schreibt nicht in dieses Log (er läuft ohne die App),
sondern nach `uebergabe-waechter.log` im App-Datenverzeichnis: Start,
„neue Version gestartet", „angekommen nach N ms" oder „Rückfall: …". Die
Lücke des Wechsels steht wie beim Installer-Weg in der Zeile
`Wiederanlauf nach Update: Tablet-Port wieder offen, Lücke N ms`.

Alle Lücken auf einmal, dem Weg zugeordnet und gegen das Erfolgskriterium
gerechnet: `node scripts/luecken-auswerten.mjs <Log-Ordner>` (siehe
[Feldtest-Protokoll](features/hotfix-ohne-neustart-feldtest.md)). Das Skript
erkennt den Weg an den Zeilen `Kernwechsel auf vX: Wächter gestartet`,
`Rückweg auf vX: Programmdatei getauscht`, `Update auf vX wird eingebaut` und
`Kernwechsel: Ankunft von vX gemeldet` —
wer deren Wortlaut ändert, macht `scripts/test-luecken-auswerten.mjs` rot.

### Wiederanlauf nach einem Update (seit v0.9.434)

Lief die Übertragung, als ein Update eingebaut wurde, startet der Kern sie
beim nächsten Programmstart selbst und misst die Unterbrechung:

| Zeile | Bedeutung |
|---|---|
| `Wiederanlauf nach Update: Übertragung vom Kern gestartet, N ms nach dem Anhalten der Vorversion` | Der Start ist angestoßen. |
| `Wiederanlauf nach Update: Tablet-Port wieder offen, Lücke N ms (Anhalten der Vorversion → Port offen)` | Die Geräte im Hallennetz erreichen den PC wieder. **Das ist die Zahl für den Vergleich** zwischen Installer-Weg und Kernwechsel. Nur mit LAN-Server; im reinen Cloud-Betrieb fehlt die Zeile. |
| `Wiederanlauf nach Update nicht möglich: …` | Die Übertragung lief, die Startbedingungen sind aber nicht erfüllt (z. B. Passwort fehlt). Der Grund steht auch im Fenster. |
| `Wiederanlauf nach Update gescheitert: …` | Der Start selbst schlug fehl. |

Die Lücke beginnt mit dem Schreiben des Wiederanlauf-Markers, also kurz vor
dem Ende der alten Version — sie ist eher etwas zu groß als zu klein.

## Automatischer Upload (Standard an, abwählbar)

Im Setup steht **„Diagnose-Logs senden"**. Ist der Schalter an, lädt
bts-light seine Logdatei alle ~10 Minuten an badhub.de hoch, damit Fehler
über alle Installationen hinweg zentral auswertbar sind.

**Bei einer Neuinstallation ist der Schalter an** (seit v0.9.325). Grund: Als
Opt-in erreichte uns ausgerechnet von den Turnieren, bei denen etwas
schiefging, meist nichts — wer im Hallenstress ein Problem hat, öffnet keine
Einstellungen. Der Assistent zeigt das Häkchen beim ersten Start gesetzt; die
Turnierleitung kann es dort **jederzeit abwählen** (Opt-out), und die
Entscheidung steht dauerhaft in der `config.json`.

Abgrenzung — wessen Einstellung wann gilt:

| Ausgangslage | `upload_logs` |
|---|---|
| Neuinstallation (keine `config.json`) | **an** — `AppConfig::neu_installation()` |
| Häkchen bewusst abgewählt | bleibt **aus** — die Datei ist die Entscheidung |
| Update einer Installation aus der Zeit vor dem Schalter (Feld fehlt) | bleibt **aus** — ein Update ist keine Neuinstallation |

- **Endpunkt:** `POST https://badhub.de/api/bts_log.php`
- **Auth:** fester Bearer-Token, in bts-light eingebacken.
- **Identifikation:** jede Installation erzeugt einmalig eine zufällige
  `install_id` (UUID); pro Installation liegt serverseitig genau eine
  Datei, jeder Upload überschreibt sie mit dem Vollstand.
- **Ablage:** `storage/bts-logs/<install-id>.log` auf dem Server.
- **Datenschutz:** die Logs enthalten nur technische Daten (Match-IDs,
  Court-Namen, BTP-Antworten, Fehler) – keine Spielernamen. Auch die
  Ansage-Ausgabe (seit v0.9.318) loggt nur Ereignisarten („Gerät neu
  geöffnet", „Rückfall") und **nie Gerätenamen** — Bluetooth-Geräte tragen
  oft den Namen ihres Besitzers.

Ist der Schalter abgewählt, bleibt das Log rein lokal. Empfänger-Seite:
[badhub `docs/features/liveticker_bts.md`](https://badhub.de) →
Abschnitt „Diagnose-Log-Upload".

## Geräte-Logs (Tablets + Pi-Monitore) — einheitlich über den PC

Tablets **und** Pi-Court-Monitore schicken ihr Verbindungslog **an den
Turnier-PC (bts-light) im LAN**; der PC legt es lokal ab und leitet es – sofern
Internet da ist – an die Cloud weiter. Vorteil: **nur der PC braucht
Internet**, minimale LTE-Daten, und der Upload läuft über **plain HTTP im LAN**
(kein TLS / keine korrekte Geräte-Uhr nötig — Pis haben keine RTC).

- **Tablet → PC:** `POST …/tablet-log?court=<id>` → lokal
  `<log_dir>/tablet-logs/court-<id>.log` → Cloud `api/tablet_log.php`
  (Geräte-ID inkl. `install_id`). Frequenz: ~30 s nach Boot, alle 5 min,
  beim Schließen/Schlafen (`pagehide`), **und sofort bei einem JS-Fehler**.
- **Court-Monitore (combo/overview/monitor) → PC:** `POST …/pi-log?device=mon-<id>`
  → lokal `<log_dir>/pi-logs/mon-<id>.log` → Cloud `api/pi_log.php`. Die
  Monitor-Seiten fangen JS-Fehler + Schlüsselereignisse („keine Daten",
  Deassign, Offline-Wechsel) und laden best-effort beim Ereignis + `pagehide`
  hoch — **seit v0.9.255 auch bei Stillstand** (siehe unten). Nur LAN (im reinen Cloud-Modus hat der Relay keine `/pi-log`-Route →
  Post scheitert still; die Kombi-Anzeige ist ohnehin LAN-only).
- **Pi → PC:** `POST …/pi-log?device=pi-<serial>` (an die gecachte
  bts-light-IP) → lokal `<log_dir>/pi-logs/pi-<serial>.log` → Cloud
  `api/pi_log.php`. Geräte-ID = **Pi-Seriennummer** (global eindeutig → ein
  Cloud-Log je physischem Pi). Frequenz: beim Boot + alle ~5 min.
- **Fire-Tablets mit der Kiosk-App → PC:** `POST …/pi-log?device=fire-<ANDROID_ID>`
  → lokal `<log_dir>/pi-logs/fire-<ANDROID_ID>.log` → Cloud `api/pi_log.php`,
  liegen also neben den `pi-…`-Dateien der Court-Monitore. Frequenz: einmal
  nach jeder erfolgreichen Suche, danach alle 5 min, solange eine Seite
  geladen ist (siehe [tablet-android-app.md](tablet-android-app.md)).
- Alles ist über **„Logs öffnen"** am PC sofort einsehbar (auch offline);
  die Cloud-Kopie liegt unter `storage/{tablet,pi}-logs/` auf badhub.de.

### Crash-Sicherheit (Tablet)

Das Tablet-Log liegt in `localStorage` (Schlüssel `badhub.tablet.log.<court>`,
bis zu 500 Einträge) und **überlebt einen Reload/WebView-Neustart**: beim
nächsten Boot wird der Puffer wieder geladen und mit hochgeladen. Zusätzlich
fängt das Tablet `window.onerror` + `unhandledrejection` als `js_error`/
`unhandled_rejection` und stößt sofort einen Upload an. *Grenze:* Wird ein
abgestürztes Tablet durch ein **anderes Gerät** ersetzt (statt neugestartet),
bleibt sein Log auf dem alten Gerät — die cloud-seitige Sicht liefert dann das
**Relay-Log** (siehe unten).

## Relay-Log (Cloud-Seite)

Im lan+cloud-Betrieb laufen Tablet-Verbindung/Übernahme/State über den
`bts-relay`-Dienst auf badhub.de. Er schreibt sein Log (neben journald)
**täglich rotierend** nach `storage/relay-logs/bts-relay.log.YYYY-MM-DD`
(Pfad aus `RELAY_LOG_DIR` in der systemd-Unit). Der `badhub`-User liest die
Datei direkt per SFTP/SSH — **ohne** `systemd-journal`-Recht. Loglevel INFO.

**Verfall (seit v0.9.387):** Es bleiben die jüngsten **30** Tagesdateien;
ältere löscht der Relay selbst — beim Start und bei jedem Tageswechsel, und
zwar **alle** Dateien, deren Name mit `bts-relay.log` beginnt (auch eine
Handkopie wie `bts-relay.log.bak`; Sicherungen also anders nennen). Lässt sich die Datei nicht anlegen, schreibt
er nur nach stdout/journald.

**Gekürzte Kennung (seit v0.9.387):** Das Relay-Log nennt von der
Turnier-Kennung (`install_id`) nur die **ersten 8 Zeichen** — die volle
Kennung ist der Zugang der Tablets und Monitore und gehört nicht in eine
Datei, die 30 Tage liegt. Ein Turnier im Relay-Log finden: die `install_id`
aus dem Diagnose-Log der App nehmen und nach ihrem Anfang suchen, etwa
`grep "'a1b2c3d4'" bts-relay.log.2026-11-14`. Dass zwei Turniere denselben
Anfang haben, ist bei 2000 Kennungen mit rund 0,05 % zu erwarten — im Zweifel
über die Uhrzeit trennen. Dateien von vor v0.9.387 nennen noch die volle
Kennung, bis sie verfallen.

Protokolliert u. a.: Tablet verbunden/getrennt, Feld belegt, Übernahme **und
ob beim Verbinden ein gespeicherter Spielstand wiederhergestellt wurde** (oder
das Feld bei 0:0 startet — die Diagnose-Zeile für den 14.06.-Vorfall).

> **Einmaliger Server-Schritt** nach dem Unit-Update (neue Env-Var):
> ```
> ssh badhub@178.104.221.177
> sudo cp /opt/bts-relay/ops/bts-relay.service /etc/systemd/system/  # bzw. aus dem Repo kopieren
> sudo systemctl daemon-reload && sudo systemctl restart bts-relay
> ```
> Das Verzeichnis `storage/relay-logs/` legt der Relay beim Start selbst an.

> Früher luden die Pis **direkt** per HTTPS in die Cloud — das scheiterte bei
> falscher Pi-Uhr (keine RTC) still an der TLS-Prüfung. Der Weg über den PC
> behebt das. (Pi-Seite: Kiosk-Skript `pi/paket/bin/bts-monitor.sh` → `log_senden`.)
