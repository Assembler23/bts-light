# Court-Monitor am Raspberry Pi einrichten

Diese Anleitung macht aus einem Raspberry Pi eine **Court-Monitor-
Anzeige**: ein TV am Spielfeld, der von selbst startet und das Spiel bzw.
Werbung zeigt. Sie ist für Einsteiger geschrieben — jeder Schritt wird
erklärt. Plane beim ersten Mal ~30 Minuten ein; jeder weitere Pi dauert
dann nur noch ein paar Minuten.

Das Feature selbst ist in [court-monitor.md](court-monitor.md) beschrieben.

## Was du brauchst

- **Raspberry Pi** (Modell 3, 4 oder 5 — ein Pi 4 ist eine gute Wahl).
- **microSD-Karte**, mind. 16 GB.
- **Netzteil** für den Pi.
- **HDMI-Kabel** zum TV. ⚠️ Pi 4 und Pi 5 haben **micro-HDMI**-Buchsen —
  dann brauchst du ein micro-HDMI-→-HDMI-Kabel oder einen Adapter.
- Einen Computer mit SD-Kartenleser für die Einrichtung.
- Das **Hallen-WLAN** (Name + Passwort).

## So funktioniert es im Überblick

Alle Monitore sind **gleich** — du musst keinen Pi fest für ein
bestimmtes Feld einrichten. Jeder Pi öffnet dieselbe Adresse, zeigt einen
**Kopplungs-Code**, und in bts-light weist du dem Code dann ein Feld zu.
Defekter Pi? Neuen anstecken, im Tool zuweisen — fertig.

## Schritt 1 — SD-Karte mit Raspberry Pi OS bespielen

Wir nutzen das **offizielle Raspberry Pi OS**. Das Bespielen übernimmt der
**Raspberry Pi Imager** — und der nimmt dir gleich WLAN und Spracheinstellung
ab, damit der Pi später ohne Tastatur ins Netz kommt.

1. Auf deinem Computer den **Raspberry Pi Imager** von
   <https://www.raspberrypi.com/software/> herunterladen und installieren.
2. SD-Karte in den Computer stecken, Imager öffnen.
3. **„Modell wählen"** → dein Pi-Modell.
4. **„OS wählen"** → *Raspberry Pi OS (64-bit)* — die normale Variante
   **mit Desktop** (nicht „Lite"; der Desktop bringt den Browser mit).
5. **„SD-Karte wählen"** → deine Karte.
6. Auf **„Weiter"** klicken — der Imager fragt **„Einstellungen
   anpassen?"** → **„Einstellungen bearbeiten"**. Das ist der wichtige
   Teil:
   - **Hostname:** ein sprechender Name, z. B. `monitor-1`. So erkennst
     du den Pi später im Netzwerk wieder.
   - **Benutzer:** Benutzername + Passwort vergeben und **merken**.
   - **WLAN einrichten:** Hallen-WLAN-Name (SSID) + Passwort eintragen,
     Land auf `DE`. → Dadurch verbindet sich der Pi beim ersten Start
     **automatisch** mit dem WLAN.
   - **Spracheinstellung:** Zeitzone `Europe/Berlin`, Tastatur `de`.
   - Reiter **„Dienste"**: **SSH aktivieren** (mit Passwort) — praktisch,
     falls du den Pi später ohne Tastatur fernsteuern willst. Optional.
7. Speichern, **„Ja"** zum Schreiben. Das dauert ein paar Minuten.

## Schritt 2 — Pi anschließen und starten

1. SD-Karte in den Pi stecken.
2. Pi per HDMI mit dem TV verbinden, TV auf den richtigen HDMI-Eingang
   stellen.
3. Netzteil anstecken. Der Pi startet (beim allerersten Mal dauert es
   etwas länger) und zeigt nach kurzer Zeit den **Desktop**.

Der Pi ist jetzt im WLAN. Wenn du eine Maus/Tastatur angeschlossen hast,
gut — für Schritt 3 brauchst du sie einmalig.

## Schritt 3 — Kiosk-Modus einrichten

Jetzt machen wir aus dem normalen Desktop die automatische Vollbild-
Anzeige. Dafür gibt es ein fertiges Skript.

1. Auf dem Pi-Desktop das **Terminal** öffnen (das schwarze Symbol oben
   in der Leiste, oder Menü → Zubehör → Terminal).
2. Das Einrichtungs-Skript herunterladen:
   ```
   curl -fsSLO https://raw.githubusercontent.com/Assembler23/bts-light/main/pi/setup-monitor.sh
   ```
   (Du kannst es vorher mit `cat setup-monitor.sh` ansehen — es ist kurz
   und kommentiert.)
3. Skript ausführen:
   ```
   bash setup-monitor.sh
   ```
4. Es fragt nach der **Monitor-Adresse**. Die findest du in bts-light
   unter **Dashboard → Court-Monitore** ganz oben — z. B.
   `http://192.168.1.50:8088/monitor`. Eintippen, Enter.

Das Skript installiert die nötigen Kleinigkeiten, schaltet die
Bildschirm-Abschaltung aus und richtet den automatischen Start ein.

## Schritt 4 — Neu starten

```
sudo reboot
```

Nach dem Neustart öffnet sich der Court-Monitor **von selbst im
Vollbild**. Zuerst zeigt der TV groß einen **Kopplungs-Code** (vier
Zeichen, z. B. `4F2A`) — das ist normal, das Gerät wartet auf seine
Feld-Zuweisung.

## Schritt 5 — In bts-light einem Feld zuweisen

1. In bts-light **Dashboard → Court-Monitore** öffnen.
2. Der Pi taucht in der Geräteliste auf — erkennbar am **Code**, der auf
   dem TV steht. (Unsicher, welcher Pi welcher ist? Auf
   **„Identifizieren"** klicken — der passende TV blendet groß Code und
   Feld ein.)
3. Im Dropdown das **Feld** wählen. Der Monitor schaltet binnen Sekunden
   um: Werbung, wenn das Feld frei ist; die Match-Ansicht, sobald ein
   Spiel darauf läuft.

Umstellen geht jederzeit über dasselbe Dropdown — ohne den Pi anzufassen.

## Weitere Pis

Da alle Monitore dieselbe Adresse nutzen, ist jeder weitere Pi schnell:

- **Einzeln:** Schritte 1–4 wiederholen (beim Imager einfach einen
  anderen Hostnamen vergeben, z. B. `monitor-2`).
- **Schneller per Klonen:** Eine fertig eingerichtete SD-Karte mit dem
  Raspberry Pi Imager (oder dem „SD Card Copier" auf dem Pi) auf weitere
  Karten kopieren. Jeder Pi vergibt sich beim ersten Start automatisch
  eine **eigene** Geräte-ID — du musst nichts umstellen.

## Tipps & Problemlösung

- **Die Adresse darf sich nicht ändern.** Die LAN-Adresse enthält die
  IP des bts-light-PCs. Vergib dem Turnier-PC im Router eine feste IP
  (DHCP-Reservierung) — sonst zeigt der Monitor nach einem Router-
  Neustart ins Leere. Alternativ den **Cloud-Modus** nutzen: dessen
  Adresse (`https://badhub.de/bts-relay/…/monitor`) ist immer stabil.
- **TV bleibt schwarz / „kein Signal":** anderen HDMI-Eingang am TV
  wählen; bei Pi 4/5 prüfen, ob das Kabel im **linken** micro-HDMI-Port
  (näher am USB-C-Strom) steckt.
- **Kiosk beenden** (zum Konfigurieren): Tastatur anschließen, **Alt+F4**.
  Danach bist du wieder auf dem Desktop.
- **Monitor hängt:** in bts-light unter „Court-Monitore" auf **„Neu
  laden"** klicken — das lädt die Seite des Pi neu.
- **Falsches WLAN / kein Netz:** SD-Karte erneut mit dem Imager
  bespielen und im Einstellungs-Dialog die WLAN-Daten korrigieren.
- **Pi aktualisieren:** gelegentlich `sudo apt update && sudo apt
  full-upgrade` im Terminal — hält das System sicher.

Die Anzeige selbst (Layout, Werbung, Timer) wird **nicht** am Pi
eingestellt, sondern zentral in bts-light — siehe
[court-monitor.md](court-monitor.md).
