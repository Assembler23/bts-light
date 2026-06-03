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
Änderungen ggü. Tilos Original:

1. **bts-light in `SERVERS`:** `http://bts-light.local:8088/monitor` (mDNS).
   Greift, wenn kein BTS-/CourtSpot-Server im Netz ist.
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

## Test (mit echter Hardware)

1. Tilos Image flashen, `/home/pi/startbrowser.sh` durch
   `pi/shared-startbrowser.sh` ersetzen, Pi neu starten.
2. **BTS-Fall:** BTS-Server unter `192.168.16.2:4433` läuft → Pi zeigt BTS.
3. **bts-light-Fall:** kein BTS im Netz, bts-light-Laptop im selben WLAN →
   Pi fällt auf `bts-light.local:8088/monitor` zurück, zeigt den Kopplungscode.
4. `/home/pi/startbrowser.log` zeigt, welcher Server gewählt wurde.
