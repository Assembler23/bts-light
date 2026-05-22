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
