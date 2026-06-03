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

## Lösung: Tilos Launcher um bts-light erweitern

Empfohlene Variante: **Tilos Desktop-Image als Basis**, sein `startbrowser.sh`
um bts-light ergänzen. Datei: [`pi/shared-startbrowser.sh`](../pi/shared-startbrowser.sh)
(Drop-in-Ersatz für `/home/pi/startbrowser.sh`). Änderungen ggü. Tilos Original:

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

## Offene Abstimmung mit Tilo (Koordination, nicht Technik)

- **Gemeinsame SSID:** beide Systeme im selben WLAN. Am einfachsten Tilos
  `btsaccess` für beide nutzen (dann muss der bts-light-Laptop ins `btsaccess`).
- **Owner des gemeinsamen Images:** Tilos Image bleibt Basis; die eine geänderte
  Datei (`startbrowser.sh`) pflegen wir/Tilo gemeinsam.
- **Ports/URLs bestätigen:** Image zeigt BTS auf `4433/d1` und CourtSpot auf
  `192.168.16.3`. Falls sich das ändert, `SERVERS` anpassen.
- **DHCP-Stolperstein** beim Heim-Test beachten (siehe Memory
  `project_verleihset_dhcp_conflict`): TP-Link am bestehenden Netz → Doppel-DHCP;
  im echten LTE-Verleih-Einsatz kein Problem.

## Test (mit echter Hardware)

1. Tilos Image flashen, `/home/pi/startbrowser.sh` durch
   `pi/shared-startbrowser.sh` ersetzen, Pi neu starten.
2. **BTS-Fall:** BTS-Server unter `192.168.16.2:4433` läuft → Pi zeigt BTS.
3. **bts-light-Fall:** kein BTS im Netz, bts-light-Laptop im selben WLAN →
   Pi fällt auf `bts-light.local:8088/monitor` zurück, zeigt den Kopplungscode.
4. `/home/pi/startbrowser.log` zeigt, welcher Server gewählt wurde.
