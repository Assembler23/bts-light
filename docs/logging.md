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
