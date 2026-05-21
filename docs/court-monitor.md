# Court-Monitor — TV-Anzeige am Spielfeld

> **Status: umgesetzt in v0.7.0.** Read-only TV-Anzeige pro Feld. Offen
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

Alle Routen gibt es doppelt — vom LAN-Server **und** vom Relay,
damit der Monitor in beiden Modi dieselbe Seite ist. `monitor.html`
nutzt durchweg **relative URLs**, daher funktioniert sie unter beiden
Pfaden ohne Anpassung.

| Zweck            | LAN                          | Cloud                                |
|------------------|------------------------------|--------------------------------------|
| Anzeige-Seite    | `/court/{label}/display`     | `/{ns}/court/{label}/display`        |
| Status (Poll)    | `/court/{label}/state`       | `/{ns}/court/{label}/state`          |
| Flaggen          | `/flags/{code}.svg`          | `/{ns}/flags/{code}.svg`             |
| Werbebild        | `/ads/{datei}`               | `/{ns}/ads/{index}`                  |
| Werbe-Upload     | —                            | `POST /{ns}/monitor` (Host → Relay)  |

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
  je einzeln ein-/ausblenden.

Die Monitor-Adressen je Feld stehen auf der Tablet-Spielzettel-Seite
(Zeile „Monitor") — dort zum Eintragen am Pi kopieren.

## Raspberry Pi — Kiosk-Einrichtung

1. Raspberry Pi OS installieren, Chromium ist vorhanden.
2. Mauszeiger ausblenden: `sudo apt install unclutter`.
3. Autostart auf die feldspezifische Display-URL, z. B.:
   ```
   chromium-browser --kiosk --noerrdialogs --disable-infobars \
     http://<bts-light-ip>:8088/court/Feld%203/display
   ```
4. Bildschirmschoner / DPMS deaktivieren (`xset s off -dpms`).

Der Pi steht üblicherweise im selben Hallen-LAN wie der bts-light-PC →
LAN-Adresse genügt. Ist der PC-Port gesperrt, die Cloud-Adresse
(`https://badhub.de/bts-relay/<install_id>/court/<label>/display`)
verwenden.

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
