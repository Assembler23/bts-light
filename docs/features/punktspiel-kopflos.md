# Punktspiel ohne Windows-PC: bts-light ohne Oberfläche

> **Status (22.11.2026):** umgesetzt. v0.9.464: `--kopflos <ordner>` (ein Prozess je Punktspiel). v0.9.465:
> `--kopflos-dienst <spool>` (der Dienst, der die Prozesse startet, überwacht und beendet), systemd-Unit,
> Einrichtungsskript und Workflow für die Auslieferung. **Noch nicht geschehen:** die Einrichtung auf dem Server
> (braucht Root, `ops/punktspiele-einrichten.sh`) und ein Spieltag damit. Entscheidung:
> [ADR 0108](../adr/0108-bts-light-ohne-oberflaeche-fuer-punktspiele.md), badhub ADR-0092 Schritt 4. Auftrag von Tilo
> vom 22.11.2026, ohne den `/idee`-Ablauf.

## Ziel

Ein Verein zählt ein Punktspiel ohne Windows-Rechner und ohne eigenen Router: Die Zählgeräte öffnen einen Link oder
QR-Code, die Feldvergabe läuft im Browser. Dahinter läuft dasselbe bts-light wie in der Halle, nur auf dem Server.

## Aufruf

```
bts-light --kopflos <ordner>
```

Der Ordner enthält `config/config.json`, `daten/` und `logs/`. Beim ersten Start liegt dort ein **Auftrag**
`auftrag.json`, den badhub schreibt:

```json
{"push_url": "https://badhub.de/api/live_update.php",
 "code": "ABCD-2345",
 "install_id": "<UUID, Namespace am Relay>",
 "name": "badhub (ohne PC)",
 "tl": {"id": "mein-badhub", "token": "<Zugang für das Turnierleitungs-Web>", "label": "Mein Badhub"}}
```

Der Prozess koppelt sich mit dem Code an das Punktspiel, schreibt die Konfiguration und löscht den Auftrag (der Code
gilt einmal). Bei jedem weiteren Start liest er nur die Konfiguration. Beendet wird er von außen (SIGTERM); alle
Stände liegen laufend auf der Platte.

## Was läuft, was nicht

| | |
|---|---|
| Quelle | nur `punktspiel` — eine andere Quelle in der Datei beendet den Start mit einem Fehler |
| Tablets | über den Cloud-Relay: `https://badhub.de/bts-relay/<install_id>/court/<Feld>` (Feld = 1 bis Felderzahl). Der Link genügt, es gibt keine Freigabe am Gerät |
| Monitor „Begegnung“ | über den Cloud-Relay: `https://badhub.de/bts-relay/<install_id>/info/begegnung` — alle Spiele und der Gesamtstand (seit v0.9.475). Der Prozess schickt dem Relay die Begegnung von selbst, es gibt keinen Schalter |
| Turnierleitung | Turnierleitungs-Web über den Relay: `https://badhub.de/bts-relay/tl#t=<token>` mit dem Zugang aus dem Auftrag |
| Meldungen an badhub | Feldvergabe, Ergebnis und nach jedem Zyklus der laufende Punktestand (`POST /api/ls/v1/begegnung/spiel`) |
| Liveticker | **aus** (`liveticker_aus`): ein dauerhaft geschlossenes Tor (`Strecke::geschlossen()`), kein Kanal sendet. Es braucht kein Liveticker-Passwort |
| LAN-Server, mDNS, Log-Upload, Auto-Update, Tray, Ansage | nicht vorhanden |

## Umsetzung

- `src-tauri/src/kopflos.rs`: Einstieg (`einstieg`), Auftrag, Grundkonfiguration, Kopplung. Läuft in `main.rs` **vor**
  Tauri: kein Display, keine Einzelinstanz-Sperre.
- `commands.rs`: `start_sync_kern` nimmt statt der Tauri-App einen `Ort` (Verzeichnisse und Zustand). `AppHandle`
  und `OrtKopflos` erfüllen ihn. Am Desktop ändert sich nichts: dieselben Verzeichnisse, derselbe Ablauf.
- `quelle/punktspiel.rs`: `punktestand_melden` meldet je laufendem Spiel den Stand vom Tablet, wenn er von dem
  abweicht, den badhub kennt. Aufgerufen über `quelle::nach_zyklus` aus der Sync-Schleife — für jede Punktspiel-
  Übertragung, auch am Desktop. So steht der Punktestand öffentlich an der Begegnung, auch ohne Liveticker.
- TLS: `kopflos::einstieg` legt den Anbieter für rustls fest (ring). Am Desktop erledigt das ein Tauri-Plugin.

## Betrieb

- **Ende-Codes:** `1` Start fehlgeschlagen (Auftrag fehlt, Code verbraucht, keine Kopplung, kein Zugang): kein
  Neustart, es braucht einen neuen Auftrag. `2` falscher Aufruf. `70` Absturz in der laufenden Übertragung: neu
  starten. Jede Panik beendet den Prozess — er soll nie als leere Hülle weiterlaufen.
- **Rechte:** Der Ordner und `config/`, `daten/`, `logs/` gehören nur dem Dienstbenutzer (0700), `config.json` 0600.
  In der Konfiguration steht der Zugang zum Turnierleitungs-Web im Klartext.
- **Ziel:** Der Auftrag darf nur auf badhub zeigen (`badhub.de`, `test.badhub.de`, nur https). Dorthin gehen
  Kopplungscode und Gerätetoken. Das Live- oder Testsystem wird vor der ersten Verbindung festgelegt, der Relay folgt.
- **Je Begegnung ein eigener Ordner.** Ein Auftrag neben einer fertigen Konfiguration wird verworfen.

## Reihenfolge der Meldungen

Feldvergabe, Ergebnis und Punktestand gehen nacheinander an badhub (eine Sende-Sperre) und werden erst unter der
Sperre aus dem dann aktuellen Paket berechnet. Eine Meldung trägt den ganzen Zustand eines Spiels: ein Punktestand
(„läuft“), der nach dem Ergebnis ankäme, höbe es sonst wieder auf. Der Punktestand läuft in einer eigenen Aufgabe,
höchstens eine zugleich, der Sync-Zyklus wartet nicht darauf. Nach einem Fehler pausiert er 30 Sekunden.

## Der Dienst: `--kopflos-dienst <spool>`

`src-tauri/src/kopflos_dienst.rs`. badhub legt je Begegnung einen Ordner `<spool>/<kennung>/` mit `auftrag.json` an.
Der Dienst sieht alle drei Sekunden in den Spool und hält zu jedem Ordner genau einen Prozess
`bts-light --kopflos <ordner>` am Leben:

| Lage des Ordners | Der Dienst |
|---|---|
| `auftrag.json` oder `config/config.json`, kein Prozess | startet einen |
| Prozess endet mit Code 70 oder durch ein Signal | startet neu, mit wachsender Pause (5 s bis 60 s) |
| Prozess endet mit Code 1 oder 2 (Startfehler) | legt die Marke `fehler` ab und startet nicht mehr — es braucht einen neuen Auftrag |
| Marke `ende` (von badhub) | beendet den Prozess und löscht den Ordner samt Gerätetoken und Konfiguration |
| Marke `fehler` älter als ein Tag, Ordner älter als 14 Tage | räumt ab |
| Prozess ohne Ordner | beendet ihn |

Nur Ordner, deren Name eine Kennung (UUID) ist, werden angefasst. Höchstens 40 Prozesse zugleich (badhub begrenzt
auf 30). Ein Prozess je Begegnung und nicht alle in einem: Der Kern hält seinen Zustand prozessweit, und ein Absturz
trifft so nur eine Begegnung.

## Einrichtung auf dem Server

1. `sudo bash ops/punktspiele-einrichten.sh` (einmalig): Laufzeit-Bibliothek, Benutzer `bts-punktspiele`, Spool je
   System (`/var/www/badhub_data/ls-hosts`, `…/ls-hosts-test`, `badhub:bts-punktspiele` 2770), `/opt/bts-light-host`,
   systemd-Unit `bts-punktspiele@prod` und `@test` (`ops/bts-punktspiele@.service`), sudoers-Regel für den Neustart
   durch `bts-deploy`.
2. Workflow **„Punktspiel-Host deployen“** in GitHub von Hand starten (`.github/workflows/punktspiele-deploy.yml`):
   baut bts-light für Linux, prüft den Start ohne Display, lädt die Programmdatei nach `/opt/bts-light-host/` und
   startet beide Dienste neu. Nicht während eines Spieltags: Ein Neustart unterbricht laufende Begegnungen für einige
   Sekunden, die Prozesse machen danach mit ihrem Stand weiter.
3. In **beiden** `.env` von badhub `LS_HOST_SPOOL_DIR` setzen (Prod `…/ls-hosts`, Test `…/ls-hosts-test`) und
   php-fpm neu starten. Erst dann erscheint „Ohne PC zählen“ in Mein Badhub.

Prüfen: `systemctl status bts-punktspiele@prod`, Protokoll mit `journalctl -u bts-punktspiele@prod -f`.
Rückweg nach einem Deploy: `/opt/bts-light-host/bts-light.rollback` zurückbenennen und die Dienste neu starten.

## Voraussetzungen auf dem Server

Das Programm ist dieselbe Datei wie am Desktop und hängt an den Laufzeit-Bibliotheken von GTK und WebKit, auch wenn
es sie ohne Fenster nie benutzt. Sie müssen installiert sein (`libwebkit2gtk-4.1-0`, `libgtk-3-0`,
`libayatana-appindicator3-1`). Ein Display braucht es nicht.

## Geprüft

- Rust-Tests: `kopflos::tests` (Auftrag, Grundkonfiguration, Fehlerfälle), `quelle::punktspiel::tests` (Punktestand).
- Probelauf am 22.11.2026 auf Linux ohne `DISPLAY` gegen eine nachgebaute badhub-API: koppelt, legt Token und
  Konfiguration an, fragt das Punktspiel im Takt ab, verbindet sich mit dem Relay, und der Relay liefert die
  Feldliste der Begegnung aus. **Nicht geprüft:** ein Tablet, das darüber zählt, und der Betrieb auf dem Server.

## Offen

- Einrichtung auf dem Server und ein erster Spieltag ohne PC. Ein Tablet, das über den Serverprozess zählt, ist
  noch nicht ausprobiert.
- Prozess von selbst beenden, wenn die Begegnung beendet ist oder nuLiga sie als gespielt führt (heute: Beenden in
  Mein Badhub oder die 14-Tage-Frist).
- Monitore ohne PC mit einem laufenden Spiel ansehen. Die Seiten (Feldmonitor, Tafel, Übersicht) liefert der
  Relay schon aus, badhub zeigt die Links; `grundkonfiguration()` stellt Verein statt Flagge ein (v0.9.466).
- Ansage ohne PC (am PC läuft sie über dessen Lautsprecher).
