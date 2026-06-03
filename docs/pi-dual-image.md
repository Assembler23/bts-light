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
   Sonst sind Pi (192.168.16.*) und Laptop in verschiedenen Subnetzen.
4. bts-light muss **gestartet** sein (grüner Punkt „Liveticker aktiv") und im
   **LAN-Modus** (Einstellungen → Tablet-Verbindung → LAN) — sonst läuft weder der
   `:8088`-Server noch die mDNS-Bekanntgabe.

Schnelltest ohne Pi-Tastatur: Handy ins `btsaccess`-WLAN, `http://bts-light.local:8088/monitor`
öffnen — erscheint die Kopplungsseite, findet der Pi sie genauso.

## Fertiges Image (Download) & Schreiben mit Raspberry Pi Imager

Das **fertig vorbereitete Shared-Image** (Tilos Image-Kopie + v3-Launcher +
`btsaccess`-WLAN, genau wie im 40-min-Test verifiziert) liegt zum Download bereit:

- **Image:** <https://badhub.de/download/bts-light/pi-image/bts-light-pi-shared-32gb.img.xz>
- **Prüfsumme:** <https://badhub.de/download/bts-light/pi-image/bts-light-pi-shared-32gb.img.xz.sha256>
- ~1,2 GB komprimiert (entpackt 15 GB). **Nur für 32-GB-Karten** (so im Einsatz);
  bewusst **nicht** geschrumpft/auto-expandiert → 1:1 der getestete Stand, sofort
  einsatzfähig.

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
> Launcher aktualisieren → `pi/shared-startbrowser.sh` ins Image schreiben
> (macOS: `e2fsck` → `debugfs -w write` auf `/dev/diskNs2`, uid/gid 1000, 0755),
> neu komprimieren (`xz -T0 -6`) und hochladen.

## Test (mit echter Hardware)

1. **Fertiges Image (Download oder lokal) mit Pi Imager auf eine neue Karte schreiben**
   (siehe Abschnitt oben) — oder manuell: Tilos Image flashen,
   `/home/pi/startbrowser.sh` durch `pi/shared-startbrowser.sh` ersetzen.
2. **BTS-Fall:** BTS-Server unter `192.168.16.2:4433` läuft → Pi zeigt BTS.
3. **bts-light-Fall:** kein BTS im Netz, bts-light-Laptop im selben WLAN →
   Pi fällt auf `bts-light.local:8088/monitor` zurück, zeigt den Kopplungscode.
4. `/home/pi/startbrowser.log` zeigt, welcher Server gewählt wurde.
