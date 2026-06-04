# Zähl-Tablets: Einstellungs-PIN & Kiosk-Sperre

Zwei getrennte Ebenen sorgen dafür, dass die Verleih-Tablets im Turnierbetrieb
„idiotensicher" sind:

1. **In-App-PIN (tablet.html):** schützt das Einstellungs-Menü am Tablet
   (Feldwechsel ohne QR). Reiner Bedien-Schutz.
2. **Kiosk-Sperre (Kiosk-Browser):** verhindert, dass Helfer das Tablet
   verlassen / ins Internet gehen, und deaktiviert die Android-Buttons.
   Verlassen nur per (Kiosk-)PIN.

> **Warum zwei Ebenen?** Eine Webseite kann die Android-System-Buttons NICHT
> deaktivieren und das Verlassen der App NICHT verhindern – das geht prinzipiell
> nur auf Geräte-Ebene (Kiosk-Browser/MDM). Der „raus nur per PIN"-Teil kommt
> daher zwangsläufig vom Kiosk-Browser, nicht aus unserem Code.

---

## 1) In-App-Einstellungs-PIN (Feldwechsel ohne QR)

Im Tablet-Header gibt es ein **Zahnrad ⚙**. Tippen → **PIN-Eingabe** → Menü:

- **Feld wechseln:** lädt die Feld-Liste vom Server (`GET /courts`, BTP-Feldname
  inkl. Halle bei Mehr-Hallen-Turnieren), Tippen schaltet das Tablet auf das
  Zielfeld um (lädt dessen Seite – **kein QR-Scan nötig**). Das aktuelle Feld ist
  markiert und gesperrt.
- **Vollbild ein/aus:** weiche Vollbild-Anzeige (versteckt Tabs/Adressleiste).
  Bei aktivem Kiosk-Browser nicht nötig.

**PIN setzen:** `tablet_settings_pin` in der `config.json` von bts-light
(Default **`0000`**). Nur Ziffern, max. 8. Greift ohne Neustart (Live-Config).
Reiner Bedien-Schutz gegen versehentliche Änderungen – **keine Sicherheitsgrenze**.

---

## 2) Kiosk-Sperre mit Fully Kiosk Browser (empfohlen)

[Fully Kiosk Browser](https://www.fully-kiosk.com/) (Android, Gratis-Version
reicht) lädt unsere Seite im Vollbild, blendet Navigations-/Statusleiste aus und
lässt sich nur per PIN verlassen. **Funktioniert für bts-light UND Tilos BTS** –
dank URL-Freigabeliste.

### Einrichtung pro Tablet (einmalig, Teil des Master-Setups)

1. **Fully Kiosk** installieren, als **Standard-Browser/Launcher** zulassen.
2. **Web-Content → Start URL:** je Einsatz das laufende System, z. B.
   - bts-light: `http://192.168.16.101:8088/court/<CourtID>`
   - Tilos BTS (Umpire-Panel): `http://192.168.16.2:4433/u`
   (Bei einem Turnier läuft ohnehin nur **ein** System. Alternativ zwei
   Lesezeichen „BTS"/„bts-light" anlegen.)
3. **Web-Content → URL-Freigabeliste (Allowlist)** auf **beide LAN-Server**
   begrenzen → alles außerhalb wird blockiert, **kein Internet**:
   ```
   192.168.16.101    (bts-light)
   192.168.16.2      (Tilos BTS)
   ```
   („Allowed Domains/URLs" einschalten und nur diese eintragen.)
4. **Device Management:**
   - **Kiosk-Modus aktivieren** (Vollbild, „Disable Status Bar", „Disable Nav Bar").
   - **„Kiosk Exit PIN"** setzen → Verlassen nur per PIN (= dein „raus nur per PIN").
   - **„Start on Boot"** + **„Auto Reload on Idle"/„Relaunch on Screen On"** für
     Robustheit über den Turniertag.
5. **WLAN `btsaccess`** (wie der Pi-Court-Monitor). Damit hängen Tablet, Pi und
   Server im selben Subnetz `192.168.16.*`.

> **Hinweis Nav-Bar:** Das vollständige Ausblenden der Android-Navigationsleiste
> gelingt am zuverlässigsten, wenn Fully Kiosk per ADB als *Device Owner*
> eingerichtet ist (einmalig pro Tablet). Ohne das werden Status-/Overlay-Leisten
> ausgeblendet und der Exit ist PIN-geschützt – für den Verleih in der Praxis
> ausreichend.

### Tilo muss nichts ändern
Tilos BTS-Server liefert die Umpire-Seite wie gehabt; sie wird nur in einem
gesperrten Browser geladen. Die Tablets sind **unsere** Verleih-Hardware.

---

## Zwei PINs – bewusst getrennt
- **Kiosk-Exit-PIN** (Fully Kiosk): „raus aus dem gesperrten Modus".
- **App-Einstellungs-PIN** (`tablet_settings_pin`): Feldwechsel im Tablet.

Eine Webseite kann die OS-Sperre nicht selbst aufheben → das sind zwangsläufig
zwei Mechanismen. Man kann beide auf denselben Wert setzen, dann fühlt es sich
wie einer an.

---

## Alternative ohne Extra-App: Android „App anheften"
Androids **Bildschirm anheften** (Einstellungen → Sicherheit) blockiert
Home/Zuletzt; Verlassen nur mit Geräte-PIN. **Aber:** innerhalb von Chrome käme
man weiter ins Internet (neuer Tab). Deshalb für „kein Internet" **nicht**
ausreichend – Fully Kiosk mit Allowlist ist die robuste Wahl.

---

## Offen (Cloud-Modus)
Im Cloud-Modus (Relay) gibt es die `/courts`-Feldliste noch nicht – das
Einstellungs-Menü meldet dort „Feldliste nicht erreichbar"; PIN-Gate und
Vollbild funktionieren. Für vollständigen Cloud-Feldwechsel müsste der Host die
komplette Feld-Liste an den Relay pushen (Folge-Arbeit).
