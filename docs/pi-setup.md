# Court-Monitor am Raspberry Pi einrichten

Diese Anleitung macht aus einem Raspberry Pi eine **Court-Monitor-
Anzeige**: ein TV am Spielfeld, der von selbst startet und das Spiel bzw.
Werbung zeigt. Sie ist für Einsteiger geschrieben — jeder Schritt wird
erklärt. Plane beim ersten Mal ~30 Minuten ein; jeder weitere Pi dauert
dann nur noch ein paar Minuten.

Das Feature selbst ist in [court-monitor.md](court-monitor.md) beschrieben.

## Was du brauchst

- **Raspberry Pi** — **Pi Zero 2 W**, Pi 3, Pi 4 oder Pi 5 (Pi 4/5 sind
  die bequemste Wahl, Pi Zero 2 W ein guter Kompromiss aus Preis und
  Größe). ⚠️ **Pi Zero W (1. Gen) ist NICHT geeignet** — er hat keine
  NEON-SIMD-Erweiterung, und modernes Chromium ist auf Debian Trixie /
  Pi OS Bookworm mit NEON als Pflicht kompiliert. Symptom: Chromium-
  Start schlägt mit einem Hardware-Dialog fehl, der TV bleibt mit dem
  Hinweis hängen. **Achtung Verwechslungsgefahr** Pi Zero W (1. Gen) vs.
  Pi Zero 2 W: physisch identisches Aussehen, komplett verschiedene
  Chips. Schnellster Test, wenn unsicher: `cat /proc/device-tree/model`
  auf einem laufenden Pi — Pi Zero **2** W steht da als „Raspberry Pi
  Zero 2 W", die 1. Gen als „Raspberry Pi Zero W". Pi Zero W (1. Gen)
  kann zwar **booten** (Pi OS Lite 32-bit, armv6-Kernel `kernel.img`),
  aber **nicht** den Chromium-Kiosk ausführen — daher als Court-Monitor
  unbrauchbar.
- **microSD-Karte**, mind. 16 GB (für das Master-Image-Konzept empfohlen
  32 GB+ einer Marken-Karte wie SanDisk Ultra A1/A2 — Class-10 / A1 / A2
  ist auf Random-IO optimiert und für Pi-Betrieb deutlich angenehmer).
- **Netzteil** für den Pi.
- **HDMI-Kabel** zum TV. ⚠️ Pi 4 und Pi 5 haben **micro-HDMI**-Buchsen,
  Pi Zero / Pi Zero 2 W haben **mini-HDMI** (anderer Stecker!) — dann
  brauchst du ein passendes Adapter-Kabel.
- Einen Computer mit SD-Kartenleser für die Einrichtung.
- Das **Hallen-WLAN** (Name + Passwort) — **2,4-GHz-Band aktiv**.
  Pi Zero 2 W und Pi Zero W können kein 5 GHz; falls das WLAN über
  Band-Steering nur 5 GHz anbietet (FRITZ!Box & Co.), klappt der
  Verbindungsaufbau nicht.

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
4. Es fragt nach der **Monitor-Adresse**. Einfach **Enter** drücken —
   dann gilt die Standardadresse `http://bts-light.local:8088/monitor`.
   Die funktioniert in **jedem** Turnier-WLAN (der Turnier-PC meldet
   sich unter diesem festen Namen), ganz ohne IP-Adresse.

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
- **Schneller per Klonen (Master-Image):** Eine fertig eingerichtete
  SD-Karte ist dein Master. Sie auf weitere Karten kopieren (Raspberry Pi
  Imager, oder der „SD Card Copier" auf dem Pi). Jeder Pi meldet sich
  automatisch mit seiner **eigenen Hardware-Seriennummer** — die geklonte
  Karte bleibt also je Pi eindeutig, du musst nichts umstellen.

## Tipps & Problemlösung

- **Keine feste IP nötig.** Der Turnier-PC meldet sich im Netz unter dem
  festen Namen `bts-light.local` (Technik: mDNS). Der Monitor findet ihn
  darüber — egal welche (DHCP-)Adresse der Laptop gerade hat. Du musst
  also weder im Router noch am Laptop eine feste IP einstellen.
  *Falls* der Name in einem Netz ausnahmsweise nicht auflöst, zeigt
  bts-light unter „Court-Monitore" zusätzlich die IP-Adresse als
  Rückfall an.
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
