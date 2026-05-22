# Court-Monitor — TV-Anzeige am Spielfeld

> **Status: umgesetzt.** v0.7.0 brachte die Anzeige, v0.8.0 die
> Geräte-Verwaltung (Zuweisung + Fernsteuerung aus dem Tool). Offen
> bleibt der 2-Felder-pro-TV-Modus → [roadmap.md](roadmap.md).

## Ziel

Pro Spielfeld ein TV (32"–55"), betrieben von einem **Raspberry Pi** im
Vollbild-Browser. Zwei Zustände, automatisch umgeschaltet:

- **Kein Spiel auf dem Feld** → **Werbung** (rotierende Bilder).
- **Spiel auf dem Feld** → **Match-Ansicht** (Layout „A — Geteilt").

Reine Anzeige (read-only) — der Monitor schreibt nie etwas zurück. Er
pollt im Sekundentakt einen `…/state`-Endpunkt.

## Layout „A — Geteilt"

Bildschirm waagerecht geteilt: oben Mannschaft 1, unten Mannschaft 2.

```
┌ FELD 3 ─────────────────── Herreneinzel ┐
│  [DE]  Anna Müller          ●            │
│                  davor 21    ▏ 11 ▕      │
│  ─────────────────────────────────────   │
│                  davor 18    ▏  7 ▕      │
│  [PL]  Hilde Kowalski                    │
└──────────────────── Gruppe 2 · Spiel 14 ┘
```

- **Kopfzeile:** Feldnummer + Disziplin (Herren-/Dameneinzel, Herren-/
  Damendoppel, Mixed).
- **Je Mannschaft (Bildschirmhälfte):** Landesflagge + Spielername(n) groß
  links; der **laufende Satzstand** ganz rechts am größten; abgeschlossene
  Sätze als kleinere Spalte daneben.
- **Doppel:** zwei Namen je Hälfte gestapelt, eine Flagge pro Spieler.
- **Aufschlag:** Der **Satzstand der aufschlagenden Mannschaft wird
  farblich hervorgehoben** (zusätzlich ein `●`-Marker am Spieler).
- **Fußzeile:** Runde + Spielnummer (je einzeln abschaltbar).
- Alles über `vh`/`vw`/`vmin` skaliert → füllt jeden TV 32"–55" ohne
  Anpassung.

Die Anzeige-Seite ist `src-tauri/assets/monitor.html` — eine
eigenständige HTML/CSS/JS-Datei, read-only Geschwister von `tablet.html`.

## Geräte-Modus & TV-Verwaltung

Monitore sind **generische Geräte**: Jeder Raspberry Pi öffnet *dieselbe*
Adresse (`…/monitor`) und vergibt sich beim ersten Start eine eigene,
dauerhafte Geräte-ID (im `localStorage`). Solange ihm kein Feld
zugewiesen ist, zeigt der TV groß einen **Kopplungs-Code** (die ersten
vier Zeichen der ID).

Im Tool führt die Seite **„Court-Monitore"** (Dashboard → Court-Monitore)
alle Geräte auf, die sich gemeldet haben:

- **Online-Status** je Gerät (grün, wenn der letzte Poll < 6 s her ist).
- **Feld-Zuweisung** per Dropdown — jederzeit umstellbar; der Monitor
  übernimmt das neue Feld beim nächsten Poll (~1 s im LAN, ≤ 3 s Cloud).
- **Identifizieren** — der Monitor blendet Code + Feld groß ein, damit
  man Gerät und TV zuordnen kann.
- **Neu laden** — der Monitor lädt seine Seite neu (falls er hängt).

Die Zuweisungen liegen in `monitor-assignments.json` im
App-Config-Verzeichnis und überstehen einen bts-light-Neustart.
Fernbefehle reiten auf dem normalen `…/state`-Poll mit — es gibt keinen
zusätzlichen Verbindungsweg zum Pi, daher funktioniert die Steuerung in
LAN **und** Cloud. Jeder Befehl trägt eine je Gerät hochzählende `id`;
der Monitor führt ihn genau einmal aus (auch nach „Neu laden" kein
Endlos-Reload).

**Direkt-Variante:** Wer einen Monitor fest auf ein Feld nageln will,
nutzt weiterhin `…/court/<Feld>/display` — ohne Zuweisungs-Schritt.

## Datenfluss

Der Monitor braucht **keinen neuen Datenweg** — alle Daten liegen schon
vor:

- Der LAN-Server bzw. der Relay kennt pro Feld das aktuelle Match
  (`MatchBrief`, seit v0.7.0 mit `discipline`, `matchNumber` und je
  Spieler `nationality`) und den Satzstand.
- Zählt ein Tablet das Feld, spiegelt es laufend seinen vollen
  Spielzustand (`court_state`) an den Server/Relay — darin stehen
  Aufschlag-Seite und Pause. Der Monitor liest diesen Zustand **rein
  lesend** mit.

`monitor.html` baut die Anzeige aus dem `…/state`-JSON
([`relay_proto::MonitorState`](../relay-proto/src/lib.rs)): Match-Info +
roher `court_state` + Konfiguration + Werbebild-Liste.

### Verhalten ohne `court_state` (kein zählendes Tablet)

| Wert            | Tablet zählt        | kein Tablet              |
|-----------------|---------------------|--------------------------|
| Satzstand       | live vom Tablet     | aus BTP (LAN) / 0:0 (Cloud) |
| Aufschlag       | angezeigt           | nicht angezeigt          |
| Pausen-Timer    | angezeigt           | nicht angezeigt          |

## Endpunkte

Alle Routen gibt es doppelt — vom LAN-Server **und** vom Relay, damit der
Monitor in beiden Modi dieselbe Seite ist. Der Server setzt beim
Ausliefern den Basis-Pfad (`__BASE__`) ein; `monitor.html` baut daraus
absolute URLs, unabhängig von der Verschachtelungstiefe.

| Zweck             | LAN                          | Cloud                          |
|-------------------|------------------------------|--------------------------------|
| Anzeige (Gerät)   | `/monitor`                   | `/{ns}/monitor`                |
| Status (Gerät)    | `/monitor/state?device=`     | `/{ns}/monitor/state?device=`  |
| Anzeige (fest)    | `/court/{label}/display`     | `/{ns}/court/{label}/display`  |
| Status (fest)     | `/court/{label}/state`       | `/{ns}/court/{label}/state`    |
| Flaggen           | `/flags/{code}.svg`          | `/{ns}/flags/{code}.svg`       |
| Werbebild         | `/ads/{datei}`               | `/{ns}/ads/{index}`            |
| Werbe-Upload      | —                            | `POST /{ns}/monitor`           |
| Geräte-Steuerung  | —                            | `POST /{ns}/monitor/control`   |
| Geräteliste       | —                            | `GET /{ns}/monitor-devices`    |

Im Cloud-Modus pusht der bts-light-Host die Feld-Zuweisungen + Fernbefehle
alle ~3 s (nur bei Änderung) an `…/monitor/control` und holt von
`…/monitor-devices` die Geräteliste für die „Court-Monitore"-Seite.

**Zugriffsschutz:** Alle Relay-Namespace-Routen haben bewusst kein eigenes
Token – das Zugangsmerkmal ist die 128-Bit-UUID des Namespace
(`install_id`). Wer sie kennt, kann Werbung/Zuweisungen überschreiben oder
ein „Neu laden"/„Identifizieren" auslösen; mehr nicht (die Befehle sind
ein geschlossenes Enum). Das ist dasselbe Modell wie für die Tablet- und
Werbe-Routen und für eine zugangsfreie Plug-and-play-App akzeptiert.

## Werbung (Leerlauf)

Läuft kein Spiel, zeigt der Monitor Werbung:

- Werbebilder werden **direkt im Tool** hochgeladen (Setup → Abschnitt
  „Court-Monitor"). **Ein gemeinsamer Werbesatz** für alle Monitore.
- Sie liegen im App-Datenverzeichnis unter `court-ads/`; der LAN-Server
  liefert sie aus `/ads/` aus.
- **Cloud-Modus:** bts-light lädt die Bilder nach dem Verbinden per
  `POST /{ns}/monitor` zum Relay hoch (Base64-JSON) und prüft alle 30 s
  per Fingerabdruck auf Änderungen. Ad-Änderungen erreichen Cloud-Monitore
  daher binnen ~30 s.
- Wechsel-Intervall einstellbar (Default 10 s).
- **Fallback** ohne konfigurierte Werbung: neutrale Seite mit Turniername
  und „Kein Spiel auf diesem Feld".

## Pausen-Timer (Retro-Klappanzeige)

Läuft eine Pause (`court_state.pause`), zeigt der Monitor einen
**Countdown im Split-Flap-Stil** (Klappanzeige wie eine alte
Flughafentafel). Greift bei den BWF-Satzpausen (Countdown) und bei
Behandlungspausen (ohne Countdown). Im Tool ein-/abschaltbar.

## Konfiguration

Setup-Wizard, Abschnitt **„Court-Monitor"** ([`CourtMonitorConfig`](../src-tauri/src/config.rs)):

- **Aktivieren** — blendet die Monitor-Adressen in der Oberfläche ein.
- **Werbebilder** — hinzufügen/entfernen (JPG, PNG, WEBP, GIF; ≤ 8 MB je
  Bild).
- **Wechsel-Intervall** — 3–30 s.
- **Anzeige-Optionen** — Disziplin / Runde / Spielnummer / Pausen-Timer
  je einzeln ein-/ausblenden. Eine Live-Vorschau im Setup zeigt die
  Wirkung jeder Option sofort.

Die Einrichtungs-Adresse und die Feld-Zuweisung der Geräte stehen auf der
Seite **„Court-Monitore"** (Dashboard → Court-Monitore).

## Raspberry Pi — Kiosk-Einrichtung

Ausführliche, einsteigertaugliche Schritt-für-Schritt-Anleitung:
**[pi-setup.md](pi-setup.md)**. Kurzfassung:

1. Raspberry Pi OS (Desktop) mit dem Raspberry Pi Imager bespielen –
   dort gleich WLAN voreinstellen.
2. Auf dem Pi das Skript [`pi/setup-monitor.sh`](../pi/setup-monitor.sh)
   ausführen → Kiosk-Autostart steht.
3. Neu starten → der TV zeigt einen Kopplungs-Code; in bts-light unter
   „Court-Monitore" dem Code ein Feld zuweisen.

**Feste Adresse ohne feste IP:** Im LAN-Modus meldet sich der Turnier-PC
per mDNS unter `bts-light.local` (siehe [mDNS](#mdns-bts-lightlocal)). Die
Standard-Monitor-Adresse `http://bts-light.local:8088/monitor` passt
dadurch in **jedem** Turnier-WLAN – ein Master-Image braucht keine
Anpassung. Ist der PC-Port gesperrt, die Cloud-Adresse
(`https://badhub.de/bts-relay/<install_id>/monitor`) verwenden.

## mDNS: `bts-light.local`

Im LAN-Modus gibt bts-light per mDNS (`tablet/mdns.rs`) den festen Namen
`bts-light.local` bekannt, der auf die aktuelle LAN-IP des Turnier-PCs
zeigt. Tablets und Monitore erreichen den PC darüber, **ohne seine
IP-Adresse zu kennen** – es ist keine feste IP nötig, weder im Router
noch am Laptop. Der Raspberry Pi löst `.local`-Namen über das
vorinstallierte avahi auf. Schlägt mDNS fehl (z. B. blockierende
Firewall), funktioniert die direkte IP-Adresse weiterhin.

## Flaggen

Nationalität ist ein IOC-Code (`GER`, `POL`, …). bts-light bündelt einen
SVG-Flaggensatz (`src-tauri/assets/flags/`, ins Binary kompiliert),
Anzeige per `<code>.svg`. Fehlt der Code, zeigt der Monitor den Namen
ohne Flagge. Herkunft/Lizenz: [`NOTICE.md`](../NOTICE.md).

## Lizenz-Hinweis

Visuelle Referenz war `phihag/bup` (u. a. PR #43, Einzelturnier-Display).
Davon wurde nur die **Idee** übernommen — **kein Code**, da die
bup-Lizenz unklar ist. Diese Anzeige ist eine eigenständige
Clean-Room-Umsetzung.

## Nicht umgesetzt

- **2-Felder-pro-TV-Modus** (`…/display?courts=3,4`) — siehe
  [roadmap.md](roadmap.md).
- **Pro-Feld unterschiedliche Werbung** — bewusst ein gemeinsamer Satz.
