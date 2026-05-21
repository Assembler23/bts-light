# Court-Monitor — TV-Anzeige am Spielfeld

> **Status: Konzept / geplant.** Dieses Dokument bereitet das Feature vor —
> Layout, Datenfluss, Konfiguration. Implementierung folgt nach Freigabe.
> Roadmap: [roadmap.md](roadmap.md) → „Court-Monitore".

## Ziel

Pro Spielfeld ein TV (32"–55"), betrieben von einem **Raspberry Pi** im
Vollbild-Browser. Zwei Zustände, automatisch umgeschaltet:

- **Kein Spiel auf dem Feld** → **Werbung** (rotierende Bilder).
- **Spiel auf dem Feld** → **Match-Ansicht** (gewähltes Layout unten).

Reine Anzeige (read-only) — der Monitor schreibt nie etwas zurück.

## Gewähltes Layout: „A — Geteilt"

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
  farblich hervorgehoben** (zusätzlich ein `●`-Marker am Spieler) — so ist
  von weitem sofort sichtbar, wer Aufschlag hat.
- **Fußzeile:** Runde + Spielnummer (optional abschaltbar).
- Alles über `vh`/`vw` skaliert → füllt jeden TV 32"–55" ohne Anpassung.

## Datenquelle

bts-light hat alle nötigen Daten bereits — kein neuer Datenweg. Pro Feld
liefert `tablet_overview()` ein `CourtOverview` mit:

- `match_id` (0 = kein Spiel → Werbemodus), `match_name`, `discipline`,
- `team1` / `team2` (Namen), `team1_nationalities` / `team2_nationalities`
  (für die Flaggen — kam mit den Sprachansagen dazu),
- `sets` (Satzstand, tablet-getrieben wenn ein Tablet zählt).

**Flaggen:** Nationalität ist ein ISO-Code (`GER`, `POL`, …). badhub hat
bereits SVG-Länderflaggen (`public/assets/flags/`); diese als Asset in
bts-light bündeln, Anzeige per ISO-Code → `<iso>.svg`.

## Architektur

- Eine eigene Anzeige-Seite, **read-only Geschwister von `tablet.html`** —
  vom LAN-Server **und** vom Relay pro Feld ausgeliefert (wie `tablet.html`
  heute, damit der Monitor in LAN und Cloud funktioniert).
- Route z. B. `GET /court/<label>/display` bzw. `/<ns>/court/<label>/display`.
- Die Seite bezieht den Court-Status (Match, Score) über denselben Weg wie
  das Tablet — aber rein lesend, nie sendend.
- **Raspberry Pi:** Chromium im Kiosk-/Vollbildmodus, Autostart auf die
  Monitor-URL des Feldes. Kurzanleitung kommt mit der Umsetzung.
- **2-Felder-Modus** (zwei benachbarte Felder auf einem großen TV): später,
  als `…/display?courts=3,4`. Nicht Teil der ersten Version.

## Werbung (Leerlauf)

Läuft kein Spiel (`match_id == 0`), zeigt der Monitor Werbung. Festgelegt:

- In den Einstellungen ein Abschnitt **„Court-Monitor"** — Werbebilder
  werden **direkt im Tool hochgeladen** (Bilddateien), Wechsel-Intervall
  (Default 10 s).
- **Ein gemeinsamer Werbesatz** für alle Monitore.
- Bilder liegen im App-Datenverzeichnis von bts-light und werden vom
  Server/Relay mit ausgeliefert.
- **Fallback** ohne konfigurierte Werbung: neutrale Seite mit Turniername /
  „Kein Spiel auf diesem Feld".
- Kommt ein Spiel aufs Feld, wechselt der Monitor automatisch zur
  Match-Ansicht; wird das Feld frei, zurück zur Werbung.

## Konfiguration im Tool

Neuer Einstellungs-Abschnitt **„Court-Monitor"**:

- Pro Feld die **Monitor-Adresse** anzeigen (analog zu den Tablet-Adressen
  in der Tablet-Spielzettel-Seite) — zum Eintragen am Pi.
- Werbebilder verwalten + Wechsel-Intervall.
- Anzeige-Optionen: Disziplin / Runde / Spielnummer ein-/ausblenden.

## Timer

Laufende Pausen zeigt der Monitor als **Countdown im Retro-Stil** — eine
Klappanzeige (Split-Flap) wie eine alte Flughafentafel, die herunterzählt.
Greift bei den BWF-Satzpausen und bei Behandlungspausen. Im Tool
ein- und ausschaltbar.

## Festgelegt

- **Werbung:** Upload direkt im Tool, ein gemeinsamer Werbesatz für alle
  Monitore (siehe oben).
- **Aufschlag:** wird angezeigt — Satzstand der aufschlagenden Mannschaft
  eingefärbt + `●`-Marker. Setzt voraus, dass der Aufschläger im
  Court-Status mitgeführt wird (heute nur am Tablet bekannt) — in der
  Umsetzung zu ergänzen.
- **Timer:** Teil der ersten Version, Retro-Klappanzeige (siehe oben).

## Lizenz-Hinweis

Visuelle Referenz war `phihag/bup` (u. a. PR #43, Einzelturnier-Display).
Davon wird nur die **Idee** übernommen — **kein Code** kopiert, da die
bup-Lizenz unklar ist (kommerzielle Lizenzdateien im Repo). Diese Anzeige
ist eine eigenständige Clean-Room-Umsetzung.
