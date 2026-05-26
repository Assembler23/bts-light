# Verleih-Set vorbereiten: Router & Master-Image

Dieses Dokument ist für die **Vorbereitung** eines Court-Monitor-Sets
(Router, TVs, Raspberry Pis). Es ist einmalige Arbeit; danach ist das Set
plug-and-play. Die Anleitung für die **Nutzung** eines fertigen Pi steht
in [pi-setup.md](pi-setup.md), das Feature selbst in
[court-monitor.md](court-monitor.md).

## Das Set

- **Router** (z. B. TP-Link TL-MR6400 — LTE-Router, kräftiges WLAN).
- Je TV: ein **Raspberry Pi** + SD-Karte + Netzteil + (micro-)HDMI-Kabel.
- Die **TVs**.
- Tablets sind optional — Schiedsrichter nutzen oft eigene Geräte (QR-Code).

Nicht im Set: der Turnier-Laptop. Darauf laufen BTP + bts-light; das
bringt die Turnierleitung selbst mit (beliebiger Laptop, siehe unten).

## Teil A — Router einrichten (einmal pro Router)

Der Router ist die **Konstante**: ein festes WLAN, das du zu jedem
Turnier mitbringst.

1. Router mit Strom versorgen, mit einem Computer verbinden (LAN-Kabel
   oder das Werks-WLAN; Name/Passwort stehen auf dem Geräte-Aufkleber).
2. Router-Admin-Seite im Browser öffnen — die Adresse steht ebenfalls auf
   dem Aufkleber (beim MR6400 meist `http://192.168.1.1` bzw.
   `http://tplinkmodem.net`). Mit den Werksdaten anmelden, danach ein
   **eigenes Admin-Passwort** setzen.
3. WLAN-Einstellungen („Wireless" / „Drahtlos"):
   - **Netzwerkname (SSID): `Turnier`**
   - **Sicherheit: WPA2**, ein Passwort vergeben.
   - 2,4-GHz-Band aktiviert lassen (große Reichweite in der Halle).
4. Speichern. **Mehr ist nicht nötig** — keine DHCP-Reservierung, keine
   Portfreigaben: mDNS (`bts-light.local`) regelt die Adressierung, alles
   läuft lokal.
5. Optional: LTE-SIM einlegen → der Router liefert zusätzlich Internet
   (dann funktionieren Auto-Updates und der Cloud-Modus als Rückfall).

> Wenn alle Sets bzw. Vereine dieselbe SSID `Turnier` + dasselbe Passwort
> verwenden, passt **ein** Master-Image für alle.

## Teil B — Master-Image erstellen (einmal)

Ein Pi wird einmal sauber eingerichtet; seine SD-Karte wird zum „Golden
Master", den du beliebig oft klonst.

1. **Einen Pi einrichten** nach [pi-setup.md](pi-setup.md):
   - Raspberry Pi OS (Desktop) flashen; im Imager WLAN **`Turnier`** +
     Passwort eintragen.
   - `setup-monitor.sh` ausführen; bei der Adresse **Enter** drücken →
     Standard `http://bts-light.local:8088/monitor`.
   - Neu starten und prüfen: am TV erscheint die Kopplungs-Seite. ✓
2. **Karte als Image sichern.** Die fertige Karte in den Computer stecken
   und in eine `.img`-Datei zurücklesen:
   - Windows: *Win32 Disk Imager* → „Read".
   - macOS/Linux: `sudo dd if=/dev/<karte> of=bts-monitor.img bs=4M`.
3. **Image verkleinern + komprimieren** (sonst wird aus einer 32-GB-Karte
   eine 32-GB-Datei): mit *PiShrink* (Linux) auf die belegte Größe
   schrumpfen, dann `.xz`-komprimieren → `bts-monitor.img.xz` (am Ende
   ~1–2 GB).
4. **Hosten:** die `.img.xz` in den Download-Bereich auf badhub.de legen
   (`download/bts-light/…`).

> Schritt 2–3 sind der einzige etwas technische Teil — einmalig. Beim
> ersten Mal am besten gemeinsam durchgehen.

## Teil C — Verteilen & klonen

- **Weitere Pis fürs eigene Set:** den „SD Card Copier" auf dem Master-Pi
  nutzen, oder die `.img.xz` mit dem Raspberry Pi Imager auf neue Karten
  schreiben. Jeder Pi meldet sich automatisch mit seiner **eigenen
  Hardware-Seriennummer** — nichts umzustellen.
- **Andere Vereine:** laden die `.img.xz`, flashen sie mit dem Raspberry
  Pi Imager. Steht ihr Router auf SSID `Turnier` + gleichem Passwort,
  läuft es sofort; andernfalls im Imager-Dialog ihr eigenes WLAN
  eintragen.

## In der Halle — der komplette Ablauf

Nichts wird eingetippt, kein Internet nötig:

```
Router an        →  WLAN "Turnier" steht von selbst
Laptop an        →  irgendeine DHCP-Adresse (egal),
                    BTP + bts-light starten (LAN-Modus)
                 →  bts-light meldet sich als bts-light.local
Pis an die TVs   →  joinen "Turnier", öffnen bts-light.local
                 →  Bild; Kopplungs-Code am TV
bts-light → Court-Monitore  →  jedem Code ein Feld zuweisen
```

Der Laptop darf **jeder beliebige** sein — keine feste IP, weil der Pi
bts-light über den Namen `bts-light.local` (mDNS) findet, nicht über eine
Adresse.

## Updates

Die Anzeige-Software (`monitor.html`) kommt **nicht** vom SD-Karten-Image,
sondern frisch vom Turnier-PC. Verbesserungen an der Monitor-Anzeige
erreichen die Pis also über das normale **bts-light-Auto-Update** — ein
neues Master-Image ist dafür **nicht** nötig. Ein neues Image braucht es
nur, wenn sich an der Pi-Grundeinrichtung selbst etwas ändert.

---

## Lessons Learned aus dem ersten Live-Test (2026-05-26)

Erste End-to-End-Validierung mit zwei Pi-Zero-2-W-Geräten parallel an
einer bts-light-Instanz (macOS-Host). Ergebnis: das Konzept funktioniert,
v0.9.18-Info-Monitor-Redirect schaltet zwei Pis live um — aber das
naive `dd`-Klonen aus Teil B oben hat **drei** harte Stolpersteine, die
das Builder-Skript zwingend lösen muss, bevor das Verleih-Set wirklich
plug-and-play ist.

### Was sofort lief

- Pi-Imager-Setup pro Pi: WLAN, SSH-User, Hostname über die Custom-
  Options → Pi bootet ins Netz, ist per SSH erreichbar.
- `setup-monitor.sh` auf der frischen Karte installiert X11 + Chromium +
  Kiosk-Autostart auf Pi-OS-Lite-Trixie sauber durch.
- mDNS: `bts-light.local` vom Mac wird von beiden Pis im selben Subnetz
  in <1 s aufgelöst, HTTP-Request liefert die Monitor-Seite in ~130 ms.
- v0.9.18-Redirect-Mechanik: ein Pi wechselt innerhalb von 1-2 s vom
  Court- zum Info-Display, sobald die Zuweisung in bts-light geändert
  wird.

### Stolperstein 1 — Chromium-„Less-than-1-GB-RAM"-Splash auf Pi Zero 2 W

Pi Zero 2 W hat 512 MB RAM. Der Pi-OS-Chromium-Wrapper zeigt **vor**
Anzeige der Seite eine modale Warnung („It is not recommended..."), die
sich mit `--noerrdialogs --disable-infobars` **nicht** unterdrücken
lässt. Der Pi-OS-Wrapper hat dafür eine eigene Option `--no-memcheck`,
die genau diese Splash überspringt.

**Fix in v0.9.19:** `--no-memcheck` ist jetzt dauerhaft im
Chromium-Aufruf in `pi/setup-monitor.sh`. Auf Pis ≥ 1 GB ist die Option
ein No-Op.

### Stolperstein 2 — SSH-Hostkeys nach `dd`-Klon

Pi-OS hat einen Service `regenerate_ssh_host_keys.service`, der beim
**ersten** Boot fehlende SSH-Hostkeys generiert und sich danach selbst
disabled (`systemctl disable`). Beim `dd`-Klon vom bereits gebooteten
Master wird dieser disabled-Zustand mitkopiert → der Klon hat keine
Hostkeys → SSH-Daemon startet nicht → der geklonte Pi ist nur per Ping
erreichbar, aber nicht per SSH administrierbar.

**Konsequenz:** ein naives `dd`-Klonen reicht nicht. Das Builder-Skript
muss vor dem Image-Pull `regenerate_ssh_host_keys.service` wieder
**enablen**, oder eine eigene Variante davon einrichten, die bei jedem
Boot prüft, ob Hostkeys existieren und sie sonst regeneriert.

### Stolperstein 3 — Hostname / WLAN / SSH-Key aus Pi-Imager-Custom-Options überlappen das Klon-Image

Pi Imager schreibt vor dem ersten Boot in `bootfs` ein `firstrun.sh`,
das die im Imager-Dialog gewählten Hostname/WLAN/User/SSH-Key auf das
System anwendet. Wenn das Master-Image diese Werte aber schon
**hartkodiert** drin hat, kommt es zu Überlappung: das `firstrun.sh`
läuft, aber der Hostname springt nach Reboot teilweise wieder auf den
ursprünglichen Master-Hostname (`/etc/hostname` wird zwar zur Laufzeit
gesetzt, aber durch einen Hook beim nächsten Reboot überschrieben).

**Konsequenz:** Das Builder-Skript muss vor dem `dd` aus dem Master-Pi
folgendes **rausnehmen**:

- `/etc/hostname` → generischer Default (z. B. `bts-monitor`).
- `/etc/wpa_supplicant/wpa_supplicant.conf` → leer / Vorlage.
- `/home/badhub/.ssh/authorized_keys` → leer.
- `/etc/sudoers.d/010_pi-nopasswd` (falls Pi-Imager-spezifisch) →
  generisch.
- `/etc/machine-id` und `/var/lib/dbus/machine-id` → leer (systemd
  generiert sie beim ersten Boot neu).
- Bash-History, `journalctl --rotate`, `apt-get clean`.

Erst dann wirken Pi-Imager-Custom-Options im Klon **wie bei einem
frischen Pi-OS-Image** und der Verleih-Operator füllt nur den Imager-
Dialog aus.

### Empfohlener Master-Image-Workflow (`pi/build-master-image.sh`)

Stolpersteine 2 + 3 sind durch `pi/build-master-image.sh` automatisch
gelöst. Das Skript läuft auf dem fertig eingerichteten Master-Pi
**vor** dem Klon. Es richtet einen persistierenden SSH-Hostkey-
Regenerator-Service ein, entfernt gerätespezifische Identität
(`/etc/hostname` → `bts-monitor`, `authorized_keys` leer, SSH-Hostkeys
+ `machine-id` gelöscht), behält das WLAN (für echtes Plug-and-Play)
und leert Caches/Logs.

**Master-Image herstellen (einmal):**

1. Einen Pi normal aufsetzen ([pi-setup.md](pi-setup.md)) → `setup-monitor.sh`
   ausführen → Reboot → Kiosk läuft am TV.
2. Skript ziehen + ausführen:
   ```
   ssh badhub@<master-ip> 'curl -fsSL https://raw.githubusercontent.com/Assembler23/bts-light/main/pi/build-master-image.sh | sudo bash'
   ```
   Oder lokal kopieren und `sudo bash build-master-image.sh`.
3. `sudo shutdown -h now` → grüne LED dauerhaft aus.
4. SD-Karte in einen Host (Mac/Linux), Image ziehen:
   ```
   sudo dd if=/dev/<karte> of=$HOME/bts-monitor-master.img bs=4M
   ```
5. Optional: mit *PiShrink* (Linux) auf die belegte Größe schrumpfen,
   dann `.xz`-komprimieren → final ~1-2 GB.
6. `.img.xz` hochladen (badhub.de oder GitHub Release).

**Verteilen (pro neuer Pi-Karte, ~5 Min):**

1. Pi Imager öffnen, „Choose OS → Use Custom" → das `bts-monitor.img.xz`.
2. „Choose Storage" → neue SD-Karte.
3. Schreiben (Pi-Imager-Custom-Options sind optional — beim
   Verleih-Set passen die fest hineingebrannten Werte fürs „Turnier"-
   WLAN ja schon).
4. SD ins Pi, anstöpseln. Bootet direkt ins Kiosk, meldet sich mit
   eigener Hardware-Seriennummer in bts-light.

### Aktueller Übergangsweg (ohne Master-Image, pro Pi ~20 Min)

Wenn noch kein Master-Image vorliegt oder ein Pi schnell als
Einzelgerät aufgesetzt wird:

1. **Pi Imager** öffnen, OS = Pi OS Lite 64-bit, Storage = die Karte.
2. **Customize:** Hostname (`monitor-2`, `monitor-3`, …), Username
   `badhub`, Passwort `badhub`, WLAN + Land DE, SSH-Pubkey aktivieren.
3. Karte ins Pi, booten, in der Fritzbox die IP suchen.
4. Im Host-Terminal: `ssh-copy-id badhub@<ip>` — Pi Imager überträgt
   den Pubkey unzuverlässig, deshalb manuell nachholen.
5. `scp pi/setup-monitor.sh badhub@<ip>:~/` und dann per SSH:
   `echo "badhub" | sudo -S -v && BTS_MONITOR_URL=http://bts-light.local:8088/monitor bash setup-monitor.sh`.
6. `sudo reboot` → der Pi steht als Kiosk.

Realistischer Zeitaufwand pro Pi: **~20 Minuten**, davon ~10 Minuten
Wartezeit (apt-get install).
