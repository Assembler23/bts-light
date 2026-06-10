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
- fehlgeschlagene Liveticker-Pushes

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
  (Geräte-ID inkl. `install_id`).
- **Pi → PC:** `POST …/pi-log?device=pi-<serial>` (an die gecachte
  bts-light-IP) → lokal `<log_dir>/pi-logs/pi-<serial>.log` → Cloud
  `api/pi_log.php`. Geräte-ID = **Pi-Seriennummer** (global eindeutig → ein
  Cloud-Log je physischem Pi). Frequenz: beim Boot + alle ~5 min.
- Beides ist über **„Logs öffnen"** am PC sofort einsehbar (auch offline);
  die Cloud-Kopie liegt unter `storage/{tablet,pi}-logs/` auf badhub.de.

> Früher luden die Pis **direkt** per HTTPS in die Cloud — das scheiterte bei
> falscher Pi-Uhr (keine RTC) still an der TLS-Prüfung. Der Weg über den PC
> behebt das. (Pi-Seite: `pi/shared-startbrowser.sh` → `upload_log`.)
