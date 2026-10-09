# Gemeinsames Pi-Image: BTS (Tilo) + bts-light

Ziel: **ein** Verleih-Set (Router, TVs, Raspberry Pis) bedient sowohl Tilos
BTS-Server als auch bts-light — ohne Karten neu zu flashen. Der Pi entscheidet
beim Boot, welcher Server im Netz ist, und lädt dessen Kiosk-URL.

## Kernerkenntnis (Image-Analyse 2026-06-03)

Tilos Image (`piZero2_image_autostart_16GB.img`, Raspberry Pi OS Desktop/Buster)
**macht die Server-Discovery bereits selbst** — genau der Mechanismus, den ein
gemeinsames Image braucht:

- Autologin `pi` → LXDE-Autostart (`/etc/xdg/lxsession/LXDE-pi/autostart`)
  ruft `@bash /home/pi/startbrowser.sh`.
- `startbrowser.sh`: wartet aufs Netz, geht eine `SERVERS`-Liste durch (`ping`
  auf den Host), nimmt den **ersten erreichbaren** und startet
  `chromium --kiosk <URL>`. Kein Treffer → `xmessage`-Fehlermeldung.
- WLAN fest gebrannt (`/etc/wpa_supplicant/wpa_supplicant.conf`):
  SSID `btsaccess`, PSK `tmt2024!`, `country=DE`, `scan_ssid=1`.
- Tilos Server (aus dem Image): `https://192.168.16.2:4433/d1` (+ `.26`/`.36`),
  CourtSpot `http://192.168.16.3/.../bup/#courtspot&display`.

Der Pi lädt also **nur** einen Kiosk-Browser; der gesamte Anzeige-Inhalt kommt
vom Server. Dual-Image ist damit **reine Boot-Discovery** — kein doppeltes OS,
kein App-Code auf dem Pi.

## Lösung: eine KOPIE von Tilos Image fürs Verleih-Set (Tilo ändert nichts)

Ein Image kann nur dann zwischen beiden Systemen wechseln, wenn seine
`SERVERS`-Liste **beide** Adressen kennt (BTS *und* bts-light). Tilos
**Original-Image bleibt unverändert** — wir nehmen eine **Kopie** fürs
Verleih-Set und ergänzen dort die eine bts-light-Zeile. Tilo muss nichts tun;
seine Pis/Server laufen wie gehabt. (Sein unverändertes Image pingt nur seine
BTS-Adressen `192.168.16.2:4433/d1` … und findet bts-light auf `:8088` nie —
daher MUSS die bts-light-Adresse in die SERVERS-Liste, aber nur in unserer Kopie.)

Datei: [`pi/shared-startbrowser.sh`](../pi/shared-startbrowser.sh)
(Drop-in-Ersatz für `/home/pi/startbrowser.sh` **auf der Verleih-Set-Kopie**).

**Verhalten (Dauerschleife, Auto-Reconnect):** Der Launcher gibt nicht mehr nach
einmaligem Suchen auf, sondern sucht laufend (Prüfung alle 10 s): kein Server →
sucht weiter; Server gefunden → Kiosk startet automatisch; Server wechselt
(BTS↔bts-light) → schaltet um; Chromium abgestürzt → Neustart. „Erst Pi, dann
Server" ist egal, ein System-Wechsel braucht keinen Pi-Neustart.

**Hysterese (v3):** Ein *einzelner* Aussetzer beendet den Kiosk NICHT — erst nach
`MISS_LIMIT` (3) erfolglosen Runden (~30 s). So flackert der Bildschirm bei einem
kurzen WLAN-Wackler nicht zum Desktop. Verifiziert: 40-min-Lauf am 2026-06-03 ohne
einen einzigen Kiosk-Abbruch trotz ~3 Mikro-Blips.

**Discovery von bts-light — Reihenfolge nach Zuverlässigkeit (v3):**
1. **gemerkte IP** (Datei `/tmp/btslight_ip`) — sofort, solange `:8088/health` antwortet.
2. **Subnetz-Scan** des eigenen /24 auf `:8088/health` — findet bts-light direkt am
   offenen Port, **unabhängig von mDNS**. mDNS (`bts-light.local`) war über WLAN das
   schwächste Glied: `getent hosts` blockierte im Feld **minutenlang** ohne Timeout.
3. **mDNS nur als Fallback**, immer mit `timeout 3` (kann nie wieder hängen).

Die IP wird in einer **Datei** gemerkt, nicht in einer Shell-Variablen — `discover()`
läuft über `$(…)` in einer Subshell, eine Variable wäre dort verloren.

Änderungen ggü. Tilos Original:

1. **bts-light-Discovery:** Subnetz-Scan auf `:8088/health` (primär) + mDNS-Fallback,
   siehe oben. Greift, wenn kein BTS-/CourtSpot-Server im Netz ist.
2. **Netz-Warten ohne Internet-Zwang:** Original wartet auf `ping 8.8.8.8`
   (Internet). Ein reines bts-light-LAN (Laptop+Router, kein Internet) hinge
   ewig → jetzt warten auf eine eigene IP (`hostname -I`).
3. **Stabile Geräte-Kennung** für bts-light: Pi-Seriennummer als
   `?device=pi-<serial>` (nur für die bts-light-URL; BTS unverändert).
4. Chromium bekommt zusätzlich `--disable-features=Translate,TranslateUI
   --lang=de-DE` (kein „Übersetzen"-Balken, wie im bts-light-Setup).

bts-light braucht avahi für `bts-light.local` — im Desktop-Image vorhanden.

### Ferndiagnose: Verbindungslog in die Cloud (ab v4)

Der Launcher lädt sein `/home/pi/startbrowser.log` periodisch (~5 min + sofort
beim Boot, im Hintergrund) an **`https://badhub.de/api/pi_log.php`** hoch
(Bearer-Token wie bts-light, Header `X-Device-Id: pi-<seriennummer>`). So lässt
sich **im laufenden Turnierbetrieb aus der Ferne** prüfen, ob ein Monitor-Pi
sauber verbunden ist — **ohne die SD-Karte zu ziehen**. Scheitert still, wenn
kein Internet da ist (Heim-Test ohne Uplink); im Verleih-Set mit LTE läuft's.

Auslesen (Server): `ssh badhub@… 'ls -lt /var/www/badhub/storage/pi-logs/;
cat /var/www/badhub/storage/pi-logs/pi-<serial>.log'`. Endpoint-Details:
badhub-Repo `docs/features/liveticker_bts.md`.

## Netz-Konvention (von Tilo vorgegeben, übernommen)

Tilo (Chat 2026-05-26): das Verleih-WLAN soll **`btsaccess` / `tmt2024!`**
heißen, Subnetz **192.168.16.\***. Damit joinen **dieselben Pis** automatisch
sowohl Tilos BTS-Netz als auch das bts-light-Verleih-Netz, und die Boot-
Discovery wählt den jeweils laufenden Server. bts-light bleibt bei mDNS
`bts-light.local` (kein fester IP-Zwang) — funktioniert im 192.168.16.*-Netz.
→ Im Verleih-Router (TP-Link) SSID/PSK/Subnetz entsprechend setzen.

**Tilo muss nichts ändern.** Nur falls er WILL, dass auch *seine* Pis bts-light
finden, würde er die bts-light-Zeile zusätzlich in *sein* SERVERS aufnehmen —
optional, nicht nötig fürs Verleih-Set.

Rest-Punkte:
- **Ports/URLs:** Image zeigt BTS auf `4433/d1` und CourtSpot auf `192.168.16.3`
  — in `shared-startbrowser.sh` verbatim übernommen. Ändert sich das, `SERVERS`
  anpassen.
- **DHCP-Stolperstein** beim Heim-Test beachten (Memory
  `project_verleihset_dhcp_conflict`): TP-Link am bestehenden Netz → Doppel-DHCP;
  im echten LTE-Verleih-Einsatz kein Problem.

## Inbetriebnahme & Stolpersteine (am 2026-06-03 live verifiziert)

Der Pi + Shared-Launcher funktionieren; die Hürden lagen beim **Server-Laptop**:

1. **Windows hat keine `.local`-Auflösung** (kein Bonjour). `http://bts-light.local:8088`
   geht auf dem Windows-Rechner SELBST nicht — lokal mit `http://localhost:8088/monitor`
   testen. **Pi (avahi) und Handy (iOS/Android) lösen `.local` dagegen auf** — bts-light
   announced den Namen über die Rust-Crate `mdns-sd`, unabhängig von Windows.
2. **Windows-Firewall** muss **TCP 8088** durchlassen (Tablet-/Monitor-Server, bindet
   auf `0.0.0.0`). Beim ersten Start „privates Netz zulassen" — der Installer legt die
   Regel via `installer/firewall-hooks.nsh` an (einmalige UAC bei *manueller* Installation,
   `IfSilent`-Guard → Auto-Updates fragen NICHT). UDP 5353 (mDNS) ist seit v3 **nicht mehr
   nötig**, weil der Pi per Subnetz-Scan am Port 8088 findet, nicht über `bts-light.local`.

> **Online-Punkt flackerte** (≤ v0.9.63): Der Server stufte einen Monitor schon nach 6 s
> ohne Poll als offline ein → ein WLAN-Mikro-Blip ließ den Punkt springen. Seit **v0.9.64**
> ist das Fenster `MONITOR_ONLINE_WINDOW_MS` = 20 s (relay-proto). Der Pi-Kiosk selbst war
> davon nie betroffen (eigene Hysterese), nur die Admin-Anzeige.
3. **Server-Laptop muss im `btsaccess`-WLAN** sein (nicht im Heimnetz 192.168.178.*).
   Sonst sind Pi (192.168.16.*) und Laptop in verschiedenen Subnetzen. Die
   bts-light-Kopfzeile zeigt rechts **„BTS-Netzwerk"** in Grün, wenn der Laptop
   im lokalen BTS-Netz hängt — erkannt am `btsaccess`-**WLAN** _oder_ an einer
   IP im **BTS-Subnetz 192.168.16.x** (also auch am LAN-Kabel). Hängt er in
   einem anderen Netz, steht dort grau „Kein BTS-Netz (\<WLAN-Name>)". So sieht
   man auf einen Blick, ob der Laptop im richtigen lokalen Netz ist (für den
   Cloud-Modus der Tablets ist das egal).
4. bts-light muss **gestartet** sein (grüner Punkt „Liveticker aktiv") und im
   **LAN-Modus** (Einstellungen → Tablet-Verbindung → LAN) — sonst läuft weder der
   `:8088`-Server noch die mDNS-Bekanntgabe.

Schnelltest ohne Pi-Tastatur: Handy ins `btsaccess`-WLAN, `http://bts-light.local:8088/monitor`
öffnen — erscheint die Kopplungsseite, findet der Pi sie genauso.

## Fertiges Image (Download) & Schreiben mit Raspberry Pi Imager

Das **fertig vorbereitete Shared-Image** (Tilos Image-Kopie + aktueller Launcher
+ `btsaccess`-WLAN) steht bereit:

- **Image:** <https://badhub.de/download/bts-light/pi-image/bts-light-pi.img.xz>
- **Prüfsumme:** <https://badhub.de/download/bts-light/pi-image/bts-light-pi.img.xz.sha256>
- Beides ist auf der öffentlichen Release-Seite <https://badhub.de/download/bts-light/>
  verlinkt (Block „Court-Monitore“, direkt unter dem Programm-Download) — für den
  Hallenaufbau, wo dieses Repo nicht zur Hand ist.
- ~1,0 GB komprimiert, **entpackt nur 3,85 GB** → schreibt + verifiziert schnell.
  **Wächst beim ersten Boot automatisch** auf die volle Kartengröße (PiShrink, via
  `/etc/rc.local`). Passt auf jede Karte ≥ 4 GB.
- **Launcher-Stand (2026-06-10):** Pi-Verbindungslogs gehen **an den PC**
  (`…:8088/pi-log`), nicht mehr direkt in die Cloud (kein TLS/keine Pi-Uhr nötig).
  Bei Launcher-Änderungen Image neu bauen (siehe Build-Quelle unten).

> Frühere große 1:1-Variante (`bts-light-pi-shared-32gb.img.xz`, 15 GB, nur 32-GB-
> Karten) wird **nicht mehr** angeboten — die kleine Variante ist inhaltsgleich
> und auto-wachsend.

**Schreiben mit Raspberry Pi Imager:**
1. Imager öffnen → **„Eigenes Image verwenden" / „Use custom"** → die `.img.xz`
   wählen (Imager entpackt beim Schreiben selbst, `.xz` muss nicht ausgepackt werden).
2. Ziel-SD-Karte (32 GB) wählen → schreiben.
3. **Keine** Imager-Anpassungen (Hostname/WLAN/SSH) nötig — WLAN `btsaccess` und der
   Launcher sind bereits im Image. (Diese Custom-Optionen würden hier ohnehin nicht
   greifen und könnten das gebackene `btsaccess`-WLAN überschreiben → weglassen.)
4. Karte in den Pi, einschalten. Nach Pi-OS-Boot startet der Kiosk automatisch und
   sucht den laufenden Server (BTS *oder* bts-light, siehe oben).

> Build-Quelle: Das Image ist eine Kopie von Tilos `piZero2_image_autostart_16GB`
> mit ersetztem `/home/pi/startbrowser.sh` (= `pi/shared-startbrowser.sh`).
>
> **Nur den Launcher aktualisieren** (schnellster Weg, nur diese eine Datei
> ändert sich) — direkt in der kleinen `bts-light-pi.img.xz`, per Docker
> Offset-Loop (macOS hat kein ext4/Loop; `losetup -P` legt im Container keine
> `p2`-Node an → Offset-Loop auf Partition 2):
> ```
> docker run --rm --privileged -v "$HOME/Downloads:/work" -w /work \
>   -v "$REPO/pi/shared-startbrowser.sh:/new.sh:ro" debian:bookworm-slim bash -c '
>   apt-get update -qq && apt-get install -y -qq util-linux e2fsprogs xz-utils parted
>   xz -dk -f bts-light-pi.img.xz
>   OFF=$(parted -m bts-light-pi.img unit B print | awk -F: "\$1==2{gsub(/B/,\"\",\$2);print \$2}")
>   LOOP=$(losetup -f --show -o "$OFF" bts-light-pi.img); e2fsck -p -f "$LOOP" || true
>   mkdir -p /mnt/r && mount "$LOOP" /mnt/r
>   cp --remove-destination /new.sh /mnt/r/home/pi/startbrowser.sh
>   chown 1000:1000 /mnt/r/home/pi/startbrowser.sh; chmod 0755 /mnt/r/home/pi/startbrowser.sh
>   sync; umount /mnt/r; losetup -d "$LOOP"
>   xz -T0 -6 -f -c bts-light-pi.img > bts-light-pi.NEW.img.xz; rm -f bts-light-pi.img'
> ```
> Dann `mv …NEW.img.xz bts-light-pi.img.xz`, `shasum -a 256 … > ….sha256`, beide
> nach `badhub@…:/var/www/badhub/public/download/bts-light/pi-image/` rsyncen.
>
> Komplett-Neubau (neues Pi-OS/Base): kleine Variante aus dem großen Image via
> PiShrink (`docker run --privileged … pishrink -Z -a gross.img bts-light-pi.img`).

## Raspberry Pi 3 B+ startet nicht — Firmware im Image zu alt

**Stand 09.10.2026: Ursache per Image-Analyse belegt, Abhilfe am Image
nachgestellt — auf echter Hardware noch NICHT gegengeprüft.**

**Symptom:** Eine mit dem Image beschriebene Karte startet im Pi Zero 2 W, im
neu gekauften Pi 3 Model B+ aber nicht — kein Bild, kein Fehlertext.

**Es ist keine andere Architektur.** Zero 2 W und 3 B+ haben praktisch
denselben Prozessor (vier Cortex-A53-Kerne) und starten denselben Kernel
(`kernel7.img`). Der Unterschied liegt eine Stufe davor, in der Start-Firmware.

**Was im Image steckt** (`bts-light-pi.img.xz`, Prüfsumme `06d3a9bf…`,
ausgelesen am 09.10.2026):

| Baustein | Stand im Image |
|---|---|
| Betriebssystem | Raspberry Pi OS **02.12.2020** (Buster), `issue.txt` |
| Start-Firmware (`bootcode.bin`, `start*.elf`, `fixup*.dat`) | **25.11.2020** |
| Kernel | 5.4.79 (`/lib/modules`, passt zur Boot-Partition) |
| Gerätebeschreibung 3 B+ (`bcm2710-rpi-3-b-plus.dtb`) | vorhanden |
| WLAN-Firmware 3 B+ (`brcmfmac43455-sdio.*`) | vorhanden |
| WLAN-Sperre (`/var/lib/systemd/rfkill`) | für alle Modelle aufgehoben |
| `cmdline.txt`/`fstab` ↔ Disk-Kennung `01ee8805` | stimmen überein |

Für einen 3 B+ der **alten** Platinen-Revision 1.3 fehlt also nichts. Der
3 B+ wird in neuerer Fertigung aber als **Revision 1.4** gebaut (Revisionscode
`a020d4`; die Vorgängerin trägt `a020d3`). Sie hat einen anderen
Spannungsregler-Baustein und braucht laut Raspberry-Pi-Forum
**Firmware ab September 2021**
([Forum 361383](https://forums.raspberrypi.com/viewtopic.php?t=361383),
[Forum 351185](https://forums.raspberrypi.com/viewtopic.php?t=351185)). Die
Firmware im Image ist zehn Monate älter — der Pi bleibt hängen, bevor der
Kernel überhaupt geladen wird. Dass die neu gekauften Geräte Rev. 1.4 sind, ist
naheliegend, aber nicht abgelesen — der Test unten klärt es in zwei Minuten.

Dass der Zero 2 W (erschienen Oktober 2021) mit derselben alten Firmware
läuft, ist im Image-Log belegt (`Machine model: Raspberry Pi Zero 2 Rev 1.0`
unter Firmware `2020-11-25`) — obwohl das Image keine eigene
Gerätebeschreibung für ihn mitbringt.

### Abhilfe: nur die Start-Firmware tauschen

[`pi/update-boot-firmware.sh`](../pi/update-boot-firmware.sh) ersetzt auf der
Boot-Partition ausschließlich `bootcode.bin`, `start*.elf` und `fixup*.dat`
durch den Buster-Stand `1.20220308_buster` (Firmware vom 01.12.2021). Kernel,
`*.dtb`, `config.txt` und das Root-Dateisystem bleiben unangetastet. Das Skript
prüft die Prüfsummen, **bevor** es die Karte anfasst, und sichert die alte
Firmware nach `firmware-alt/`.

**Bereits beschriebene Karten** müssen nicht neu geschrieben werden — die
Boot-Partition ist FAT und erscheint am Rechner als Laufwerk `boot`:

```
bash pi/update-boot-firmware.sh /Volumes/boot          # Mac
bash pi/update-boot-firmware.sh /media/$USER/boot      # Linux
bash pi/update-boot-firmware.sh --zurueck /Volumes/boot   # Rückweg
```

Unter Windows von Hand: die 17 Dateien aus
<https://github.com/raspberrypi/firmware/tree/1.20220308_buster/boot>
(`bootcode.bin`, alle `start*.elf`, alle `fixup*.dat`) auf das Laufwerk `boot`
kopieren und die vorhandenen ersetzen.

### Offen, bevor das veröffentlichte Image ersetzt wird

1. **3 B+ Rev. 1.4:** eine Karte patchen, starten — kommt der Kiosk?
2. **Zero 2 W gegenprüfen (Pflicht).** Die neue Firmware kennt den Zero 2 W
   und sucht eine eigene Gerätebeschreibung (`bcm2710-rpi-zero-2-w.dtb`), die
   das Image nicht hat; die alte Firmware nahm stillschweigend eine andere. Ob
   die neue genauso zurückfällt, zeigt nur der Test. Startet der Zero 2 W mit
   der neuen Firmware nicht mehr, braucht das Image zusätzlich die beiden
   `bcm2710-rpi-zero-2*.dtb` aus demselben Firmware-Stand.
3. Erst wenn **beide** Modelle starten: Image neu packen (Boot-Partition des
   Images einhängen, Skript darauf ausführen, `xz`, neue Prüfsumme) und per
   rsync ersetzen. Bis dahin bleibt das veröffentlichte Image unverändert.

Das selbst eingerichtete Image nach [pi-setup.md](pi-setup.md) (aktuelles
Raspberry Pi OS + `setup-monitor.sh`) ist **nicht** betroffen — es bringt
aktuelle Firmware mit.

## Test (mit echter Hardware)

1. **Fertiges Image (Download oder lokal) mit Pi Imager auf eine neue Karte schreiben**
   (siehe Abschnitt oben) — oder manuell: Tilos Image flashen,
   `/home/pi/startbrowser.sh` durch `pi/shared-startbrowser.sh` ersetzen.
2. **BTS-Fall:** BTS-Server unter `192.168.16.2:4433` läuft → Pi zeigt BTS.
3. **bts-light-Fall:** kein BTS im Netz, bts-light-Laptop im selben WLAN →
   Pi fällt auf `bts-light.local:8088/monitor` zurück, zeigt den Kopplungscode.
4. `/home/pi/startbrowser.log` zeigt, welcher Server gewählt wurde.
