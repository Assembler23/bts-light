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

## 2) Kiosk-Sperre mit Fully Kiosk Browser

[Fully Kiosk Browser](https://www.fully-kiosk.com/) lädt unsere Seite im Vollbild.
Läuft auch auf **Amazon Fire-Tablets** (Installation über den Amazon Appstore
oder als APK von fully-kiosk.com — kein Google Play nötig).

**Gratis vs. PLUS — wichtig (am 2026-06-04 auf Fire-Tablet verifiziert):**

| Funktion | Gratis | PLUS (einmalig/Gerät) |
|---|---|---|
| Vollbild | ✅ | |
| Bildschirm an lassen (Keep Screen On) | ✅ | |
| Start beim Booten | ✅ | |
| Web Auto Reload (Reconnect/Error) | ✅ | |
| **Kiosk Mode** (App-Verlassen sperren, Buttons sperren, **Exit-PIN**, Single-App) | ❌ | ✅ |
| **Web-Filter / URL-Allowlist** (kein Internet) | ❌ | ✅ |

→ **Die eigentliche Sperre (kein Internet + Buttons gesperrt + Exit-PIN) braucht PLUS.**
Gratis bekommt man eine saubere, dauerhafte Anzeige, aber die Android-Buttons
bleiben aktiv (Helfer *könnten* die App verlassen). Für ein echtes Verleih-Set
daher **PLUS pro Gerät** (Volumen-Rabatt für ~10 Tablets) einplanen.

### Einrichtung pro Tablet (einmalig, Teil des Master-Setups)

1. **Fully Kiosk** installieren (Fire: Amazon Appstore / APK), öffnen.
2. **Start URL** (Quick-Start oder Web Content Settings) je Einsatz, z. B.
   - bts-light (empfohlen, ab v0.9.94): **`http://<PC-IP>:8088/felder`** — die
     **Felder-Lobby**: zeigt alle Felder, ein Tipp startet das Zählen, belegte
     Felder sind als „belegt" markiert (Doppelbelegung bleibt ausgeschlossen).
     Kein QR, kein Feld pro Tablet vorkonfigurieren.
   - bts-light (fest auf ein Feld): `http://<PC-IP>:8088/court/<CourtID>`
   - Tilos BTS (Umpire-Panel): `http://192.168.16.2:4433/u`
   ⚠️ `<PC-IP>` = die **aktuelle** LAN-IP des Turnier-PCs (nicht fest 192.168.16.101);
   am einfachsten den QR aus dem „QR-Codes"-Tab nutzen. Bei einem Turnier läuft
   ohnehin nur **ein** System. Feld später wechseln: über die Lobby bzw. das
   ⚙-PIN-Menü — kein QR.
3. **Gratis-Robustheit:** *Device Management* → **Keep Screen On** + **Start on
   Boot**; *Web Auto Reload* → **Reload on Network Reconnect** + **on Error** (NICHT
   „on Idle" — würde mitten im Spiel neu laden).
4. **Nur mit PLUS:** *Advanced Web Settings → Web Filter* → nur `192.168.16.101`
   und `192.168.16.2` erlauben (kein Internet); *Kiosk Mode* → **Exit-PIN** setzen,
   „Disable Nav/Status Bar", Single-App.
5. **WLAN `btsaccess`** (wie der Pi-Court-Monitor) → selbes Subnetz `192.168.16.*`.

> **Hinweis Fire-Tablet:** Die Android-Navigationsleiste vollständig auszublenden
> gelingt zuverlässig nur mit PLUS (Kiosk Mode) bzw. *Device Owner* per ADB.

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

## Cloud-Modus
Der Feldwechsel funktioniert **auch im Cloud-Modus** (ab v0.9.67): Der Host
pusht die Feld-Liste an den Relay (`HostFrame::Courts`), der sie unter
`/{ns}/courts` ausliefert. Der Cloud-PIN ist dort technisch bedingt `0000`
(der Relay kennt den Host-PIN nicht) – im LAN-Modus gilt der konfigurierte PIN.
