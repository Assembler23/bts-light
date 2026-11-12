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
- **Eigenes Netzteil** für den Pi — **nicht** die USB-Buchse des TVs
  (siehe [„Strom und Einschalten"](#strom-und-einschalten)). Pi Zero 2 W
  und Pi 3B+: offizielles Micro-USB-Netzteil 5,1 V / 2,5 A.
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

   > **Bewusst `http`, nicht `https`.** Der Turnier-PC bietet seit
   > [ADR 0047](adr/0047-lan-tls-konkretisierung.md) auch einen
   > verschlüsselten Zugang auf Port 443 an — für die Pis ist der aber weder
   > vorgesehen noch nötig. Sie haben keine Echtzeituhr und würden ein
   > Zertifikat bei falsch gestellter Uhr **still** verwerfen; und die
   > Zertifikatswarnung ließe sich im Kiosk-Betrieb ohne Tastatur nicht
   > bestätigen. Port 8088 bleibt deshalb dauerhaft offen. Details:
   > [court-monitor.md](court-monitor.md).

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

## Ton für Ansagen am TV (seit v0.9.313)

Der Monitor kann die Ansagen seiner Halle mitsprechen — über den Lautsprecher
des TVs. Dafür in `bts-monitor-url.txt` hinter die Adresse
`?ansage=<Halle>` hängen, zum Beispiel:

```text
http://bts-light.local:8088/monitor?ansage=Halle%20A
```

Ein Leerzeichen im Hallennamen schreibst du als `%20` (das Startskript
entfernt Leerzeichen). Bei nur einer Halle genügt `?ansage=`. Am Turnier-PC
muss auf der Seite **Ansagen** der Schalter **Audio für Ansage-Seiten
erzeugen** an sein, und für Sprache am Pi braucht es Azure. Mehr dazu:
[court-monitor.md](court-monitor.md) „Ansage-Modus".

Damit der Pi **ohne Tippen** Ton abspielen darf, startet das Kiosk-Skript
Chromium mit `--autoplay-policy=no-user-gesture-required`.
<!-- pruef: "--autoplay-policy=no-user-gesture-required" in pi/setup-monitor.sh -->

**Pis, die vor v0.9.313 eingerichtet wurden, haben dieses Flag noch nicht.**
Sie zeigen im Ansage-Modus unten links „Ton gesperrt …" und sprechen nicht.
Abhilfe: das aktuelle `setup-monitor.sh` einmal neu herunterladen und
ausführen (Schritt 3) — Achtung, es fragt die Adresse neu ab und überschreibt
`bts-monitor-url.txt`; gib dort gleich die Adresse mit `?ansage=…` ein. Oder
die Karte mit einem neuen Master-Image bespielen.

**Kein Ton am TV?** Im Pi-Desktop über das Lautsprecher-Symbol bzw. mit
`sudo raspi-config` → *System Options* → *Audio* den HDMI-Ausgang wählen.

## Strom und Einschalten

Ziel: **Steckdosenleiste an → TV zeigt die Anzeige**, ohne Fernbedienung,
ohne Umstecken.

**Eigenes Netzteil, nicht der USB-Anschluss am TV.** Die USB-Buchse eines
Fernsehers liefert meist nur 0,5–0,9 A und schaltet im Standby oft ab. Ein
Pi 3B+ braucht mit Chromium über 1 A, ein Pi Zero 2 W beim Hochfahren
ebenfalls mehr, als die Buchse hergibt. Folgen: Der Pi startet nicht, hängt
oder läuft erst nach Aus- und Einstecken des Kabels. Netzteil und TV am besten
an **dieselbe Steckdosenleiste** — dann starten beide gemeinsam.

Ob ein Pi zu wenig Strom bekommen hat, zeigt im Terminal
`vcgencmd get_throttled`: `throttled=0x0` heißt in Ordnung, jeder andere
Wert (z. B. `0x50005`) heißt, die Spannung ist eingebrochen.

**Der Pi schaltet den TV selbst ein (HDMI-CEC).** Das Einrichtungsskript
installiert einen kleinen Dienst, der nach dem Start den TV weckt und ihn auf
den Eingang des Pi schaltet. Er wiederholt das bis zu drei Minuten, bis der
TV „an" meldet — Fernseher sind nach dem Einstecken unterschiedlich schnell
bereit. Kommt der TV später aus dem Standby (Fernbedienung), meldet sich der
Pi noch einmal als aktiver Eingang. Wer bewusst auf einen anderen Eingang
schaltet, wird **nicht** zurückgeholt.
<!-- pruef: "bts-cec.service" in pi/setup-monitor.sh -->

Dafür muss CEC **am TV eingeschaltet** sein. Der Menüpunkt heißt je Marke
anders: Samsung **Anynet+**, LG **SimpLink**, Sony **Bravia Sync**,
Philips **EasyLink**, Panasonic **VIERA Link**, sonst meist „HDMI-CEC" oder
„HDMI-Gerätesteuerung".

- **Abschalten** für eine Karte (z. B. ein TV, der damit Unsinn macht): auf
  dem Karten-Laufwerk eine leere Datei `bts-cec-aus.txt` anlegen. Wirkt
  ab dem nächsten Start des Pi.
  <!-- pruef: "bts-cec-aus.txt" in pi/setup-monitor.sh -->
- **Was tut der Dienst gerade?** `journalctl -u bts-cec` im Terminal.

Das Skript setzt außerdem `vc4.force_hotplug=1` in `cmdline.txt`: Der Pi gibt
auch dann ein Bild aus, wenn der TV beim Booten noch dunkel war. (Das ältere
`hdmi_force_hotplug=1` in `config.txt` wirkt auf aktuellem Pi OS nicht mehr,
bleibt aber für alte Images stehen.)
<!-- pruef: "vc4.force_hotplug=1" in pi/setup-monitor.sh -->

**Ein Image für Pi Zero 2 W und Pi 3B+:** Beide laufen mit demselben
aktuellen **Raspberry Pi OS (64-bit)**. Karte für einen der beiden
einrichten, als Master-Image klonen ([pi-master-image.md](pi-master-image.md))
und auf alle Pis spielen.

## Nur-Lese-Modus: Strom einfach abziehen

Nach dem Turnier wird das Set einfach vom Strom genommen. Damit die SD-Karte
das auf Dauer übersteht, läuft der Pi im **Nur-Lese-Modus**: Alles, was er
während des Betriebs schreibt, liegt nur im Arbeitsspeicher und ist nach dem
Neustart weg — die Karte selbst wird nicht verändert. Die Feldzuweisung bleibt
trotzdem erhalten, denn der Pi meldet sich immer mit seiner
Hardware-Seriennummer.

Geschaltet wird über eine Datei auf dem **Karten-Laufwerk** (die Partition,
die auch ein Windows-PC sieht, dort liegt auch `bts-monitor-url.txt`):

| Datei `bts-schreiben-erlaubt.txt` | Zustand nach dem nächsten Start |
|---|---|
| liegt dort | Karte **beschreibbar** (zum Einrichten, für Updates) |
| fehlt | Karte **schreibgeschützt** (Turnierbetrieb) |
<!-- pruef: "bts-schreiben-erlaubt.txt" in pi/setup-monitor.sh -->

Das Umschalten erledigt ein kleiner Dienst (`bts-nur-lesen.service`) beim
Start: Er stellt den Modus um und **startet den Pi dafür einmal von selbst
neu** — das dauert etwa zwei Minuten länger als sonst. Danach ist Ruhe.
<!-- pruef: "bts-nur-lesen.service" in pi/setup-monitor.sh -->

- **Nach `setup-monitor.sh`** liegt die Datei dort — ein frisch
  eingerichteter Pi bleibt also beschreibbar, bis du sie löschst (am
  Computer oder mit `sudo rm /boot/firmware/bts-schreiben-erlaubt.txt`).
- **Beim Master-Image** löscht `build-master-image.sh` die Datei selbst:
  Jede damit bespielte Karte richtet beim ersten Start WLAN/Hostname aus dem
  Raspberry Pi Imager ein und schaltet sich danach schreibgeschützt.
- **Etwas ändern** (Update, anderes WLAN, Skript neu ausführen): Datei
  anlegen, Pi starten, warten, bis er sich ein zweites Mal neu gestartet
  hat — dann ist die Karte beschreibbar. Danach Datei wieder löschen.
  `setup-monitor.sh` verweigert im Nur-Lese-Modus den Dienst, damit keine
  Änderung still verloren geht.
- **Klappt das Umschalten zweimal nicht,** gibt der Dienst auf (sonst würde
  der Pi endlos neu starten). Ansehen mit `journalctl -u bts-nur-lesen`;
  neuer Versuch: Datei `bts-nur-lesen-versuche.txt` auf dem Karten-Laufwerk
  löschen.

## Wie der Pi den Turnier-PC findet

Das Kiosk-Skript sucht den Turnier-PC **selbst** und gibt nicht auf: zuerst
die zuletzt gefundene Adresse, dann alle Geräte im eigenen Netz auf
`:8088/health`, zuletzt den Namen `bts-light.local`. Ist der PC noch nicht da,
sucht es laufend weiter (eine Suchrunde dauert etwa 20–30 Sekunden); stürzt
Chromium ab, startet es ihn neu; ist der PC drei Runden lang (gut eine Minute)
weg, schließt es den Kiosk und sucht weiter. Kurze WLAN-Aussetzer lassen die
Anzeige also stehen. Die Reihenfolge beim Einschalten ist also egal.

Steht in `bts-monitor-url.txt` die Standardadresse mit `bts-light.local`,
übernimmt das Skript daraus nur Pfad und Zusätze (etwa `?ansage=Halle%20A`)
und setzt die gefundene IP ein. Eine andere Adresse (feste IP, Cloud) wird
genau so verwendet, wie sie dort steht.

Das Verbindungsprotokoll geht alle fünf Minuten an den Turnier-PC, der es an
die Cloud weiterreicht ([logging.md](logging.md)). Gerätekennung ist
`pi-<Seriennummer>` — dieselbe wie beim früheren gemeinsamen Image, die
Feldzuweisung bleibt beim Neubespielen also erhalten. Nur Pis, die vorher mit
einem älteren `setup-monitor.sh` eingerichtet waren (Kennung = letzte acht
Stellen der Seriennummer), erscheinen einmal mit neuem Code und müssen neu
zugewiesen werden.

## Tipps & Problemlösung

- **Keine feste IP nötig.** Der Turnier-PC meldet sich im Netz unter dem
  festen Namen `bts-light.local` (Technik: mDNS). Der Monitor findet ihn
  darüber — egal welche (DHCP-)Adresse der Laptop gerade hat. Du musst
  also weder im Router noch am Laptop eine feste IP einstellen.
  *Falls* der Name in einem Netz ausnahmsweise nicht auflöst, zeigt
  bts-light unter „Court-Monitore" zusätzlich die IP-Adresse als
  Rückfall an.
- **Oben steht „Deutsch / Englisch" (Übersetzen-Leiste):** bts-light auf
  dem Turnier-PC auf v0.9.392 oder neuer aktualisieren und den Pi einmal
  neu starten — die Seiten lehnen die Übersetzung seitdem selbst ab. Am Pi
  ist dafür nichts neu einzurichten.
- **TV bleibt schwarz / „kein Signal":** anderen HDMI-Eingang am TV
  wählen; bei Pi 4/5 prüfen, ob das Kabel im **linken** micro-HDMI-Port
  (näher am USB-C-Strom) steckt.
- **TV schaltet nicht von selbst auf den Pi:** CEC im TV-Menü einschalten
  (siehe [„Strom und Einschalten"](#strom-und-einschalten)); dann
  `journalctl -u bts-cec` ansehen. Steht dort „Eingang: nicht gesendet
  (Adresse unbekannt)", erkennt
  der Pi den TV über HDMI nicht — anderes Kabel oder anderen Eingang probieren.
- **Pi startet nur nach Aus- und Einstecken:** fast immer zu wenig Strom —
  eigenes Netzteil statt TV-USB.
- **Pi startet beim Einschalten zweimal:** normal, wenn der Nur-Lese-Modus
  gerade umgeschaltet wird (siehe oben).
- **Änderungen am Pi sind nach dem Neustart weg:** Der Nur-Lese-Modus ist an
  — erst `bts-schreiben-erlaubt.txt` anlegen.
- **Kiosk beenden** (zum Konfigurieren): Tastatur anschließen, **Alt+F4**.
  Danach bist du wieder auf dem Desktop.
- **Monitor hängt:** in bts-light unter „Court-Monitore" auf **„Neu
  laden"** klicken — das lädt die Seite des Pi neu.
- **Falsches WLAN / kein Netz:** SD-Karte erneut mit dem Imager
  bespielen und im Einstellungs-Dialog die WLAN-Daten korrigieren.
- **Pi aktualisieren:** gelegentlich `sudo apt update && sudo apt
  full-upgrade` im Terminal — hält das System sicher. Vorher den
  Nur-Lese-Modus ausschalten, sonst ist das Update nach dem Neustart weg.

Die Anzeige selbst (Layout, Werbung, Timer) wird **nicht** am Pi
eingestellt, sondern zentral in bts-light — siehe
[court-monitor.md](court-monitor.md).
