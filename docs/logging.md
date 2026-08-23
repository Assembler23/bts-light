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
- fehlgeschlagene Liveticker-Pushes
- **Perf-Zeile der Anzeige-Strecke** (seit v0.9.235, siehe unten)

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

## Automatischer Upload (opt-in)

Im Setup lässt sich **„Diagnose-Logs senden"** aktivieren. Dann lädt
bts-light seine Logdatei alle ~10 Minuten an badhub.de hoch, damit Fehler
über alle Installationen hinweg zentral auswertbar sind.

- **Endpunkt:** `POST https://badhub.de/api/bts_log.php`
- **Auth:** fester Bearer-Token, in bts-light eingebacken.
- **Identifikation:** jede Installation erzeugt einmalig eine zufällige
  `install_id` (UUID); pro Installation liegt serverseitig genau eine
  Datei, jeder Upload überschreibt sie mit dem Vollstand.
- **Ablage:** `storage/bts-logs/<install-id>.log` auf dem Server.
- **Datenschutz:** die Logs enthalten nur technische Daten (Match-IDs,
  Court-Namen, BTP-Antworten, Fehler) – keine Spielernamen.

Ohne den Schalter bleibt das Log rein lokal. Empfänger-Seite:
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
  hoch. Nur LAN (im reinen Cloud-Modus hat der Relay keine `/pi-log`-Route →
  Post scheitert still; die Kombi-Anzeige ist ohnehin LAN-only).
- **Pi → PC:** `POST …/pi-log?device=pi-<serial>` (an die gecachte
  bts-light-IP) → lokal `<log_dir>/pi-logs/pi-<serial>.log` → Cloud
  `api/pi_log.php`. Geräte-ID = **Pi-Seriennummer** (global eindeutig → ein
  Cloud-Log je physischem Pi). Frequenz: beim Boot + alle ~5 min.
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
> behebt das. (Pi-Seite: `pi/shared-startbrowser.sh` → `upload_log`.)
