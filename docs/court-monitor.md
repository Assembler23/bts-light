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
- **Schriftgrößen** (seit v0.9.145, Turnier-Feedback „Namen zu klein"):
  Nachname 9.2vmin (Doppel 6.4), Vorname 3.6vmin (Doppel 3), laufender
  Satz 13vmin. Lange Namen kürzen weiterhin per Ellipsis statt umzubrechen.

Die Anzeige-Seite ist `src-tauri/assets/monitor.html` — eine
eigenständige HTML/CSS/JS-Datei, read-only Geschwister von `tablet.html`.

## Geräte-Modus & TV-Verwaltung

Monitore sind **generische Geräte**: Jeder Raspberry Pi öffnet *dieselbe*
Adresse (`…/monitor`). Pi-Monitore melden sich mit ihrer CPU-Seriennummer
(`device=pi-<serial>`); ohne `?device` vergibt sich die Seite beim ersten
Start eine eigene, dauerhafte Geräte-ID (im `localStorage`). Solange dem
Gerät kein Feld zugewiesen ist, zeigt der TV groß einen **Kopplungs-Code**
(die **letzten** vier alphanumerischen Zeichen der ID). Bewusst das Ende:
alle Pi-Serials beginnen mit demselben Präfix (`00000000…`), die ersten
vier Zeichen wären sonst für jeden Pi gleich („PI00") und nicht
unterscheidbar.

Im Tool führt die Seite **„Court-Monitore"** (Dashboard → Court-Monitore)
alle Geräte auf, die sich gemeldet haben:

- **Online-Status** je Gerät (grün, wenn der letzte Poll < 6 s her ist).
- **Feld-Zuweisung** per Dropdown — jederzeit umstellbar; der Monitor
  übernimmt das neue Feld beim nächsten Poll (~1 s im LAN, ≤ 3 s Cloud).
- **Identifizieren** — der Monitor blendet Code + Feld groß ein, damit
  man Gerät und TV zuordnen kann. Wirkt in **allen** Anzeigen — Einzelfeld
  (`monitor.html`), Court-Übersicht (`overview.html`) und Kombi
  (`combo.html`) (seit v0.9.93; davor nur Einzelfeld).
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

| Zweck                  | LAN                            | Cloud                          |
|------------------------|--------------------------------|--------------------------------|
| Anzeige (Gerät)        | `/monitor`                     | `/{ns}/monitor`                |
| Status (Gerät)         | `/monitor/state?device=`       | `/{ns}/monitor/state?device=`  |
| Anzeige (fest)         | `/court/{label}/display`       | `/{ns}/court/{label}/display`  |
| Status (fest)          | `/court/{label}/state`         | `/{ns}/court/{label}/state`    |
| **Court-Übersicht**    | `/info/overview`               | (LAN-only erstmal)             |
| **In Vorbereitung**    | `/info/preparation`            | (LAN-only erstmal)             |
| Vorbereitungs-Daten    | `/info/preparation/state`      | (LAN-only erstmal)             |
| Flaggen                | `/flags/{code}.svg`            | `/{ns}/flags/{code}.svg`       |
| Werbebild              | `/ads/{datei}`                 | `/{ns}/ads/{index}`            |
| Werbe-Upload           | —                              | `POST /{ns}/monitor`           |
| Geräte-Steuerung       | —                              | `POST /{ns}/monitor/control`   |
| Geräteliste            | —                              | `GET /{ns}/monitor-devices`    |

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
  daher binnen ~30 s. **Ops-Hinweis:** nginx muss für `/bts-relay/`
  `client_max_body_size` ≥ 25 MB setzen, sonst scheitert der Upload mit
  HTTP 413 (Standardwert 1 MB ist zu klein).
- Wechsel-Intervall einstellbar (Default 10 s).
- **Fallback** ohne konfigurierte Werbung: neutrale Seite mit Turniername
  und „Kein Spiel auf diesem Feld".
- **Abschaltbar:** Die Option „Werbung im Leerlauf anzeigen" steuert, ob
  ein freies Feld überhaupt Werbung zeigt. Aus → das Feld zeigt immer die
  neutrale Leerlauf-Seite, auch wenn Werbebilder hinterlegt sind.

## Spieldauer-Anzeige

Zählt ein Tablet das Feld, kennt der Monitor den Spielbeginn
(`court_state.startedAt`) und zeigt optional neben der Feldnummer die
laufende Spieldauer in Minuten (Stoppuhr-Symbol). Im Tool ein-/abschaltbar.
Ohne zählendes Tablet bleibt die Anzeige leer.

## Entschiedenes Match (kein Geister-Satz)

Endet ein Best-of-3 in zwei Sätzen, schickt das Tablet die Sätze plus
einen leeren dritten 0:0-Eintrag — frühere Monitor-Versionen zeigten den
als „laufenden Satz" als ob ein dritter Satz käme. Sobald das Tablet im
gespiegelten `courtState` `finished: true` meldet, schaltet der Monitor
auf die **Endergebnis-Ansicht** um:

- Ein etwaiger 0:0-Geistersatz am Ende fällt weg.
- Alle wirklich gespielten Sätze werden als „fertig" gerendert; der
  große laufende-Satz-Box entfällt komplett. Die Done-Sätze werden in
  dieser Ansicht etwas größer gesetzt (`.scores.decided .set-done`),
  damit das Endergebnis aus der Distanz lesbar ist.
- Pro Satz wird das Gewinner-Team hell hervorgehoben (`.set-done.won`,
  Verlierer bleibt gedämpft).
- Die Sieger-Hälfte bekommt einen grünen Akzentbalken (`.half.winner`)
  und eine 🏆-Markierung.
- Der Aufschlag-Indikator (`serving`) ist in dieser Ansicht unterdrückt.

Sieger-Bestimmung in [monitor.html](../src-tauri/assets/monitor.html)
(`matchWinner`):

- Bei Aufgabe (`courtState.retired === true`) → `retiredWinner`
  (`'a'`/`'b'`).
- Sonst → Team mit den meisten Satzgewinnen (`a > b` zählt für Team 1).

Per-Satz-Hervorhebung wird bei einer Aufgabe absichtlich **nicht**
angewendet — der letzte Satz ist dort unvollständig, die Punkte-Mehrheit
ist daher kein zuverlässiger Satzgewinner. Der Match-Sieger (🏆 + grüne
Hälfte) bleibt korrekt.

Eingeführt in v0.9.15.

## Spielernamen (Broadcast-Stil)

Namen werden zweizeilig dargestellt: Vorname(n) klein darüber, Nachname
groß darunter — wie in Sport-Übertragungen. Der letzte Namensteil gilt
als Nachname. So bleibt der Nachname auch bei langen Doppel-Namen aus der
Distanz gut lesbar, der Vorname geht nicht verloren, und das Bild ist für
alle Spieler:innen einheitlich. Ein einteiliger Name steht ohne Vornamen-
Zeile; ein sehr langer Einzelteil wird mit „…" abgeschnitten.

## Layout

Das Anzeige-Layout ist im Setup wählbar. Aktuell gibt es **„A — Geteilt"**
(Team 1 oben, Team 2 unten); die Auswahl ist die Grundlage für weitere
Layouts. Der Monitor setzt das gewählte Layout als `data-layout` am
Wurzelelement.

## Pausen-Timer (Retro-Klappanzeige)

Läuft eine Pause (`court_state.pause`), zeigt der Monitor einen
**Countdown im Split-Flap-Stil** (Klappanzeige wie eine alte
Flughafentafel). Greift bei den BWF-Satzpausen (Countdown) und bei
Behandlungspausen (ohne Countdown). Im Tool ein-/abschaltbar.

## Konfiguration

Setup-Wizard, Abschnitt **„Court-Monitor"** ([`CourtMonitorConfig`](../src-tauri/src/config.rs)):

- **Aktivieren** — blendet die Monitor-Adressen in der Oberfläche ein.
- **Werbung im Leerlauf anzeigen** — steuert, ob ein freies Feld Werbung
  zeigt oder die neutrale Leerlauf-Seite.
- **Werbebilder** — hinzufügen/entfernen (JPG, PNG, WEBP, GIF; ≤ 8 MB je
  Bild).
- **Wechsel-Intervall** — 3–30 s.
- **Layout** — Anzeige-Layout des Monitors (aktuell „A — Geteilt").
- **Kombi-Anzeige: Felder nebeneinander** (`combo_vertical`, seit v0.9.97) —
  zeigt bei der Kombi-Anzeige zwei Felder **nebeneinander** (Hochformat je Feld:
  Team 1 oben, Spielstand als Satz-Paare mittig, Team 2 unten) statt
  über­einander. Sinnvoll, wenn ein TV zwischen zwei Feldern steht. Der Server
  hängt dann `&dir=v` an die Kombi-URL (`/combo?courts=…&dir=v`). Globaler
  Schalter (gilt für alle Kombi-Anzeigen).
- **Anzeige-Optionen** — Disziplin / Runde / Spielnummer / Spieldauer /
  Pausen-Timer je einzeln ein-/ausblenden. Eine Live-Vorschau im Setup
  zeigt die Wirkung jeder Option sofort.

Die Einrichtungs-Adresse und die Feld-Zuweisung der Geräte stehen auf der
Seite **„Court-Monitore"** (Dashboard → Court-Monitore).

## Kombi-Anzeige (`combo.html`)

Mehrere Felder auf einem TV (bis zu 3), als Bänder über- oder (mit
`combo_vertical`) nebeneinander. Datenquelle ist `/combo/state` —
derselbe `overview()`-Stand wie die Einzelanzeige. **Nur LAN:** der Relay
transportiert nur Einzelfeld-Zuweisungen, Kombi-Monitore laufen daher über
den Turnier-PC.

- **Satz-Sieger deutlich hinterlegt** (seit v0.9.105): Der gewonnene Satz
  steht nicht nur weiß/grau, sondern als **grüner Block** (`.set.won` /
  `body.vertical .vset.won`) — aus der Ferne sofort als Sieger erkennbar
  (Feld-Wunsch 2026-06-15). Laufender Satz bleibt gelb (`.current`).
- **Pausen-Countdown am betroffenen Feld** (seit v0.9.105): Läuft an einem
  Feld eine Pause (`court_state.pause`), zeigt das Band dieses Felds die
  Restzeit (`Pause`/`Satzpause` + `m:ss`, `Behandlung` ohne Countdown) —
  „an der Seite, wo die Pause ist". `combo.html` rechnet den Countdown
  relativ zur Server-Zeit (`serverNowMs` im `/combo/state`-Payload), weil
  die Pi keine synchrone Uhr haben muss; das Tablet setzt `endsAt` in
  Server-Zeit. Das Feld `CourtOverview.pause` wird in `overview()` 1:1 aus
  dem Tablet-`court_state` übernommen (wie `serving`).

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

## Info-Monitor (Hallen-Display)

Neben dem feld-bezogenen Court-Monitor (ein TV pro Feld) liefert bts-light
zwei **Hallen-weite Info-Anzeigen** unter dedizierten URLs aus — ideal für
ein Display am Halleneingang oder am Schiedsrichter-Tisch der TL. Beide
nutzen denselben Tablet-Server, brauchen also weder Internet noch
badhub.de.

> **TV-Launcher (Tippen sparen):** An einem Smart-TV ohne feste Zuweisung muss
> man nur die **kurze** Adresse `http://bts-light.local:8088` (oder `/tv`) tippen
> — es erscheint ein **Auswahl-Menü**, das man mit der **Fernbedienung
> (Pfeiltasten + OK)** bedient. Es bietet **Lokal** (bts-light: „Alle Hallen",
> je Halle ein Button, „Nächste Spiele") **und Online** (öffentlicher
> badhub-Liveticker je Halle, etwas andere Darstellung — aus dem konfigurierten
> Verband). Kein `?halle=` tippen. Direkt-Kurzpfade ohne Menü: `…/alle`,
> `…/h/1`, `…/h/2` (n-te Halle, alphabetisch), `…/next`. (Pi-Monitore brauchen
> gar nichts zu tippen — die werden im Tool zugewiesen.)

| URL | Was es zeigt |
|---|---|
| `http://bts-light.local:8088/info/overview` | **Court-Übersicht** — alle Felder mit Status („frei" / „läuft" / „Behandlung" / „TL"), aktuellem Spiel, Paarung und Sätzen. Bei Doppeln stehen die zwei Partner untereinander (wie der badhub-Hallen-Monitor). Bei Mehr-Hallen-Turnieren ohne `?halle=` **rotiert** die Anzeige automatisch durch die Hallen (jede einzeln im Vollbild). |
| `http://bts-light.local:8088/info/preparation` | **In Vorbereitung** — Liste der gerufenen und eingeplanten Spiele; aufgerufene mit gold-Pille „In Vorbereitung", Halle und „vor X Min." hervorgehoben. |

Beide Seiten verstehen zwei URL-Parameter:

- **`?halle=<Name>`** — filtert auf eine Halle. Court-Übersicht zeigt nur
  die Felder dieser Halle; Vorbereitungs-Monitor nur die Aufrufe für diese
  Halle. Vergleich getrimmt + case-insensitiv. Beim Court-Grid: kein
  Treffer → alle Felder (Tippfehler-Schutz). Beim Vorbereitungs-Monitor:
  kein Rückfall, der Operator soll explizit sehen, wenn nichts für die
  Halle gerufen ist.
- **`?rotate=90|180|270`** — Pivot-/Hochformat-Monitore: rotiert die
  gesamte Anzeige per CSS-Transform im Browser. Pi-OS-seitig keine
  Änderung nötig (kein `xrandr`, kein `display_rotate=` in config.txt).
  `0` oder weggelassen = normal.
- **`?hallSeconds=<n>`** — nur Court-Übersicht: Intervall der **Hallen-
  Auto-Rotation** in Sekunden (Default 12, min 3). Greift nur, wenn mehrere
  Hallen erkannt werden und **kein** `?halle=` gesetzt ist.

> **Links nicht von Hand bauen:** Die bts-light-Seite **Court-Monitore** zeigt
> unter „Court-Übersicht (Hallen-Display)" die fertigen Links automatisch — den
> öffentlichen Online-Liveticker, die lokale Gesamt-Übersicht und (ab 2 Hallen)
> je Halle einen `?halle=`-Link zum Kopieren auf den Hallen-TV. „Öffnen" zeigt
> die Vorschau am PC.
>
> **Pi direkt einer Halle zuweisen:** Im Zuweisungs-Dropdown eines Geräts
> stehen ab 2 Hallen unter „Informationen" automatisch „Court-Übersicht – alle
> Hallen" **und** je Halle „Court-Übersicht – Halle X". Wählt man eine Halle,
> wird der Pi fest auf `…/info/overview?halle=<Halle>` umgeleitet — kein
> manuelles URL-Eintippen am Pi nötig.

**Mehr-Hallen-Verhalten der Court-Übersicht (ein TV pro Halle ODER ein TV für
alle):**

- **Fester TV pro Halle:** `…/info/overview?halle=Halle%201` → zeigt dauerhaft
  nur diese Halle im Vollbild (bei 12 Feldern ein 4×3-Raster). Empfohlen, wenn
  pro Halle ein Display vorhanden ist.
- **Ein TV für mehrere Hallen:** `…/info/overview` (ohne `?halle=`) → erkennt
  mehrere Hallen und **wechselt automatisch** durch sie (Halle 1 Vollbild →
  nach `hallSeconds` Halle 2 → …). Der Kopf zeigt den Hallennamen + „1 / N".

Beispiele:

- Eingangs-TV Halle 1 im Pivot: `…/info/preparation?halle=Halle%201&rotate=90`
- Fester Court-TV Halle 2: `…/info/overview?halle=Halle%202`
- Ein TV, alle Hallen im 20-Sek-Wechsel: `…/info/overview?hallSeconds=20`

Eingerichtet wird das nach dem Pi-Standardablauf
([pi-setup.md](pi-setup.md)) — nur die `bts-monitor-url.txt` auf der
Boot-Partition zeigt nicht auf `/monitor`, sondern auf die passende
`/info/…`-Variante.

## Siegerehrung (Sieger-Monitor)

Eigener Menüpunkt **„Siegerehrung"** in der App (neben „Monitore"). Dort wählt
der Operator live, welche ausgespielte Disziplin auf dem Sieger-Monitor
erscheint (keine Rotation — ideal zum Fotografieren des Podiums). Die
Disziplin-Auswahl ist global (`set_winners_selection`/`winners_overview`), wirkt
also auf alle Sieger-Monitore gleichzeitig.

Die TV-**Zuweisung** bleibt unter „Monitore": ein Gerät bekommt „Siegerehrung —
ganzes Podium" oder „nur Platz 1/2/3" (drei Einzel-TVs vor dem Podest).

Anzeige (`winners.html`):

- Endpunkte `GET /info/winners` (ganzes Podium) bzw. `?only=1|2|3` (ein Platz je
  TV); Zustand über `GET /info/winners/state` (Disziplinen, `selected`,
  `tournament`).
- Podium: Namen zweizeilig (Vorname / Nachname), mehrere Vornamen gekürzt.
- Einzel-Modus: ganzer Name in **einer Zeile**, per `fitSolo()` dynamisch auf
  ~94 % der Breite skaliert (kurze Namen durch die Höhe begrenzt) — nutzt die
  **volle Breite** statt fixer `vmin`-Größen. Verein größer dargestellt.
- Layout = Flex-Spalte (wie `overview.html`): Header / `main` / Footer jeweils
  über die **volle Breite**. Footer zweizeilig: **Turniername** (klein) über der
  **Disziplin** (groß).
- **Vereinslogos** neben dem Vereinsnamen (sofern in Badhub vorhanden):
  - Quelle: `GET {base}/api/v1/club-logos` (key-frei, **verbandsübergreifend**) →
    `{clubs:[{name, logo_url}]}`. `base` = Origin aus `badhub.url` (kein Slug
    nötig → auch Teilnehmer aus anderen LVs bekommen ihr Logo). clubfinder war
    geo-/verbandsgebunden, `/federations/…/clubs` braucht einen Key.
  - Backend `tablet/club_logos.rs` matcht den BTP-Vereinsnamen (exakt → lose ohne
    Klammerzusatz; mehrdeutige lose Treffer werden verworfen) und cacht
    Vereinsliste (6 h / 60 s bei Fehler) + Bildbytes; Endpoint
    `GET /info/club-logo?name=…` liefert das Bild lokal aus (auch für LAN-TVs ohne
    Internet). SSRF-sicher: Bild-Origin == badhub-Origin; Slug streng validiert.
  - Kein Treffer / kein Logo / offline → `<img onerror>` entfernt sich, es bleibt
    **nur der Name** (kein Platzhalter).
- Sonderfall „zwei dritte Plätze" (kein Spiel um Platz 3): `?only=3` zeigt beide
  Paare kompakter (`multi`-Modus).

## mDNS: `bts-light.local`

Im LAN-Modus gibt bts-light per mDNS (`tablet/mdns.rs`) den festen Namen
`bts-light.local` bekannt, der auf die aktuelle LAN-IP des Turnier-PCs
zeigt. Tablets und Monitore erreichen den PC darüber, **ohne seine
IP-Adresse zu kennen** – es ist keine feste IP nötig, weder im Router
noch am Laptop. Der Raspberry Pi löst `.local`-Namen über das
vorinstallierte avahi auf. Schlägt mDNS fehl (z. B. blockierende
Firewall), funktioniert die direkte IP-Adresse weiterhin.

**Verifikation 2026-05-25:** Test mit Raspberry Pi (Pi OS Lite 32-bit,
avahi-daemon) im FRITZ!Box-WLAN; bts-light-Bekanntmachung am Mac per
`dns-sd -P bts-light _bts-light._tcp local 8088 bts-light.local. <ip>`
simuliert → vom Pi mit `avahi-resolve -n bts-light.local` aufgelöst →
korrekte IP zurück, auch über die WLAN↔Ethernet-Bridge der FRITZ!Box
hinweg. Der frühere Fehlversuch von einem Windows-PC war ein
Windows-Client-Problem (Windows ist als mDNS-Client unzuverlässig), nicht
ein bts-light-Problem.

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
