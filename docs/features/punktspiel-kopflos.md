# Punktspiel ohne Windows-PC: bts-light ohne Oberfläche

> **Status (22.11.2026):** Teil 1 umgesetzt mit v0.9.464 — bts-light startet mit `--kopflos <ordner>` ohne Fenster,
> koppelt sich selbst an ein Punktspiel und hängt sich als Host an den Cloud-Relay. **Noch nicht gebaut:** der Dienst
> auf dem badhub-Server, der je Begegnung einen solchen Prozess startet und beendet, und die Bedienung in Mein Badhub
> (Links für Tablets und Turnierleitung). Entscheidung: [ADR 0108](../adr/0108-bts-light-ohne-oberflaeche-fuer-punktspiele.md),
> badhub ADR-0092 Schritt 4. Auftrag von Tilo vom 22.11.2026, ohne den `/idee`-Ablauf.

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

- Dienst auf dem badhub-Server: je Begegnung starten und beenden, Auslieferung des Programms, Benutzer und Rechte.
- Mein Badhub: „Ohne PC zählen“ mit Links und QR-Codes für Felder und Turnierleitung.
- Ende: Prozess beenden, wenn die Begegnung beendet ist oder nuLiga sie als gespielt führt.
