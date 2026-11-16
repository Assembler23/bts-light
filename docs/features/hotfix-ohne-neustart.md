# Hotfix ohne Neustart — Spezifikation

> Status: **Freigegeben 2026-11-16** (via /idee: Brief → Grill → How-To → Review). Umsetzung noch nicht begonnen.
> Quelle: Idee vom 16.11.2026 („Wir trauen uns nicht, einen Fix im laufenden
> Turnier einzuspielen, weil bts-light dafür neu starten muss").
> Betroffene Crates: `src-tauri`, `src` (Frontend), CI-Workflows. `relay/` und
> `relay-proto/` bleiben unverändert.
> ADR: [0099](../adr/0099-geraete-seiten-zur-laufzeit-tauschen.md) ·
> [0100](../adr/0100-kernwechsel-ohne-installer.md) ·
> [0101](../adr/0101-rueckweg-und-datenformat-regel.md).
> Baut auf: [update-im-turnierbetrieb.md](update-im-turnierbetrieb.md) (ADR 0057) ·
> [tablet-version-abgleich.md](tablet-version-abgleich.md).

## Kontext / Problem

Fällt während eines Turniers ein Fehler auf und ist der Fix fertig, wird er
heute meist **nicht** eingespielt. Zwei Gründe:

1. **Die Lücke.** Ein Update heißt Installer-Lauf plus Prozess-Neustart.
   LAN-Server (8088/8443), Relay-Verbindung und Monitor-Sockets sind solange
   weg. Die Roadmap nennt „~20 s"; gemessen hat das niemand.
2. **Kein Zurück.** Macht der Fix etwas anderes kaputt, gibt es am Turnier-PC
   keinen Weg zur vorigen Version.

Dazu kommt ein Umstand, der beim Planen auffiel: Die Geräte-Seiten
(Zähl-Tablet, TL-Web, Monitore …) sind fest in die Programmdatei
einkompiliert (`tablet/assets.rs`). Selbst ein Ein-Zeilen-Fix an
`tablet.html` braucht im LAN-Betrieb einen Neustart der ganzen App. Im
Cloud-Betrieb ist derselbe Fix Minuten nach dem Merge live, weil das Relay
eigene Kopien ausliefert und bei jedem Merge auf `main` deployt.

Was heute schon trägt: Tablets zählen offline weiter und schieben nach,
Ergebnisse liegen in der BTP-Retry-Queue auf Platte, die Übertragung läuft
nach einem Update von selbst wieder an (ADR 0057), die Automatik-Pause der
Feldvergabe liegt seit v0.9.323 in der Config, das Relay hält 20 s ohne Host
aus (`HOST_KARENZ_MS`).

## Zielbild & Erfolgskriterien

Zwei Wege, ein Rückweg — alles per Banner am Turnier-PC, nichts ändert sich
ohne Klick.

**Weg 1 — Seiten tauschen (keine Lücke).** Die App ersetzt die ausgelieferten
Geräte-Seiten zur Laufzeit durch ein signiertes **Seiten-Paket**. Server,
Sockets, BTP- und Relay-Verbindung laufen weiter.

**Weg 2 — Kernwechsel ohne Installer (wenige Sekunden).** Die neue
Programmdatei wird im Hintergrund neben die laufende gelegt und geprüft. Der
Wechsel ist: Übertragung anhalten, zwei Dateien umbenennen, neuen Prozess
starten. Ein Wächter stellt die alte Version wieder her, wenn die neue nicht
anläuft.

**Rückweg.** Genau ein voriger Stand bleibt liegen — für die Seiten immer,
für den Kern, wenn das Release es erlaubt. Ein Klick stellt ihn wieder her.

Erfolgskriterien:

1. Ein reiner Seiten-Fix ist im LAN einspielbar, ohne dass eine einzige
   Verbindung abreißt.
2. Die Lücke eines Kernwechsels („Port zu → Port wieder offen") wird von der
   App selbst in Millisekunden geloggt. Am Referenz-PC (Windows 11, Defender
   an), 10 Läufe: **Median ≤ 3 s, Maximum ≤ 5 s**, und höchstens ein Drittel
   des im selben Verfahren gemessenen Installer-Wegs.
3. Nach jedem Wechsel läuft die Übertragung ohne Klick weiter; eine gesetzte
   Automatik-Pause ist danach noch gesetzt.
4. Startet die neue Version nicht, läuft ohne Zutun wieder die alte, mit
   Übertragung und einer verständlichen Meldung.
5. Die Turnierleitung liest im Banner, was der Fix kostet („ohne
   Unterbrechung" / „wenige Sekunden"), wen er erreicht und wie sie
   zurückkommt — ohne Handbuch.
6. Installationen ohne dieses Feature aktualisieren unverändert über
   `latest.json`.

## Nicht-Ziele

- Prozess-Übergabe ohne jede Lücke (Socket-Handover, Kern als Dienst) —
  bleibt Roadmap „Stufe 3".
- Fern-Schalter für Features; Fern-Einspielen durch Entwickler; Freigabe je
  Installation; eigener Kanal zum Einschalten.
- Automatisches Zurückrollen bei **fachlichen** Fehlern. Einzige Ausnahme ist
  der Start-Wächter („neue Version startet nicht").
- Neue Auto-Neulade-Wege für TL-Web, Meetingpoint, Ansage, Lobby, Anzeige,
  Vorbereitung, Spielfolge. Sie holen einen Seiten-Fix beim nächsten Neuladen
  von Hand.
- Seiten-Tausch für Cloud-Geräte. Deren Seiten kommen weiter vom Relay.
- Verteilung eines Seiten-Stands vom Master an Slave-PCs. Jeder PC spielt für
  sich ein.
- Seiten-Pakete aus `main`. Sie entstehen nur auf Hotfix-Zweigen (siehe E2).
- Hotfix-Zweige mit Rust-Änderungen. Ein Kern-Fix ist ein normales Release.
- Archiv älterer Versionen. Es gibt genau einen vorigen Stand.
- Kernwechsel ohne Installer unter Linux/macOS und bei einer
  MSI-Installation; dort bleibt der bisherige Weg.
- Umstellung von „Beim Beenden einbauen". Das bleibt der stille Installer
  (ADR 0057).
- Härtung von `latest.json` (`require_signed_version`). Als eigener Punkt in
  die Roadmap.
- Persistenz weiterer RAM-Zustände (siehe „Bekannte Grenzen").

## Entscheidungen

### E1 — Auslöser und Verteilung

Die App lädt im Hintergrund vor und zeigt ein Banner; eingespielt wird nur
per Klick am Turnier-PC. Angebote gehen an **alle** Installationen, deren
Kern-Version passt. Es gibt keinen zweiten Kanal und keine Freigabe je
`install_id`.

### E2 — Seiten-Pakete entstehen auf einem Hotfix-Zweig ab dem Tag

`main` läuft den Release-Tags voraus (16.11.2026: Tag v0.9.421, danach neun
Merges, acht davon mit Rust-Änderungen). Ein Paket aus `main` enthielte
Seiten, die Funktionen eines Kerns voraussetzen, den der Turnier-PC nicht hat.

Deshalb:

- Ein Hotfix liegt auf dem Zweig **`hotfix/vX.Y.Z`**, der beim Tag `vX.Y.Z`
  abzweigt. Der Fix wird dorthin übernommen (Cherry-Pick) **und** regulär nach
  `main` gemergt — letzteres bringt ihn über den Relay-Deploy auch zu den
  Cloud-Geräten und in das nächste Release.
- Den Paket-Job startet ein Maintainer **von Hand auf `main`** und nennt den
  Tag. Workflow und Bau-Skript kommen so aus `main`; der Hotfix-Zweig wird
  nur als Daten gelesen (über git, nie ausgecheckt oder ausgeführt).
  *Geändert am 16.11.2026 nach dem Security-Review von PR 3:* Die erste
  Fassung („jeder Push auf `hotfix/v*` startet den Job") hätte Workflow und
  Skript aus dem Zweig selbst laufen lassen — wer dort schreiben darf, hätte
  die Liste fälschen können, die der Freigebende liest.
- Der Job **bricht ab**, wenn
  sich gegenüber dem Tag ein Kern-Pfad geändert hat. Kern-Pfade sind
  `src-tauri/**` außer `src-tauri/assets/*.html`, dazu `relay-proto/**`,
  `Cargo.toml`, `Cargo.lock`.
- Das Paket gilt für **genau die Kern-Version des Tags**. Das ist die
  Präzisierung der Grill-Entscheidung „Paket nennt Mindest-Kern": Ein Zweig ab
  dem Tag trägt die Seiten dieses Tags; auf einem neueren Kern würde er dessen
  Seiten zurückdrehen. Den Wert setzt weiterhin die CI, nie ein Mensch.
- Reihenfolge innerhalb einer Kern-Version: **`folge`** = Zahl der Commits
  auf dem Hotfix-Zweig seit dem Tag (`git rev-list --count vX.Y.Z..HEAD`),
  ganzzahlig, streng wachsend. Der eingebaute Stand hat Folge 0.

### E3 — Signiert wird erst nach Freigabe durch einen Maintainer

Bisher signiert nur der Release-Lauf, den allein ein Tag-Push des Admins
auslöst. Der Paket-Job prüft und baut zur Probe sofort und zeigt den
Unterschied der Seiten gegenüber dem Tag; vor dem Signieren wartet er in
einem geschützten GitHub-Environment (`seiten-hotfix`, nur von `main` aus)
auf die Freigabe eines Maintainers. Signiert wird nur, wenn der Bau danach
dieselben Bytes ergibt wie der Probebau. Erst dann wird veröffentlicht.

### E4 — Umfang und Reichweite von Weg 1

- **Alle 17 Geräte-Seiten** unter `src-tauri/assets/*.html` sind tauschbar.
  Flaggen und andere Nicht-HTML-Assets nicht.
- Neuladen wie heute: Tablets über den vorhandenen Versionsabgleich (freies
  Tablet lädt selbst, Tablet im Spiel zeigt nur den Hinweis), Monitore über
  den vorhandenen Befehl „Neu laden", alle übrigen Seiten von Hand.
- Der Tausch wirkt auf alles, was **dieser PC** selbst ausliefert — auch als
  Slave-Träger. Im reinen Cloud-Betrieb gibt es kein Seiten-Angebot.
- Die Desktop-Oberfläche (React) ist keine Geräte-Seite; ein Fix dort ist
  immer Weg 2.

### E5 — Rückweg

- **Seiten:** immer möglich, ohne Lücke. „Zurück" aktiviert den vorigen Stand
  (das vorige Paket oder den eingebauten Stand) und sperrt die
  zurückgenommene Folge.
- **Kern:** nur, wenn das Release die Vorversion als lesefähig ausweist
  (ADR 0101). Sonst ist der Knopf sichtbar, aber aus, mit Begründung.
- Eine zurückgenommene Version oder Folge wird **nicht erneut angeboten**;
  eine neuere schon.
- Der Knopf steht im Banner direkt nach dem Einspielen und dauerhaft auf der
  Wartungsseite.

### E6 — Start-Wächter

Meldet sich die neue Version nach einem Kernwechsel nicht innerhalb der Frist
als angekommen, stellt der Wächter die alte Programmdatei wieder her, startet
sie und die Übertragung läuft wieder an. Die Version ist danach gesperrt.

### E7 — Wiederanlauf im Kern

Der Wiederanlauf der Übertragung nach einem Update wandert vom WebView
(`App.tsx` `wiederAnlaufen`) in den Rust-Kern (`.setup()`). Das verkürzt die
Lücke beider Wege und ist Voraussetzung für die Messung.

### E8 — „Beim Beenden einbauen" bleibt

Unverändert der stille Installer. Damit der Rückweg trotzdem existiert,
kopiert die App vor jedem Installer-Lauf die laufende Programmdatei als
Vorversion.

## Betroffene Komponenten / Architekturregeln / Daten

- **Crates/Komponenten:**
  - `src-tauri/src/tablet/assets.rs` — aus 17 `pub const …_HTML` und dem
    `OnceLock` der Marke wird ein austauschbarer **Seitenstand**.
  - `src-tauri/src/tablet/server.rs`, `tablet/slave_bridge.rs` — alle
    Aufrufer der Konstanten (rund 25 Stellen), Pong-Antworten, Signal „Port
    gebunden".
  - neu `src-tauri/src/seiten_paket.rs` — Format, Prüfung, Ablage, Zustand.
  - neu `src-tauri/src/signatur.rs` — minisign-Prüfung.
  - neu `src-tauri/src/uebergabe.rs` — Tausch-Plan, Wächter, Probelauf,
    Zustandsdateien; Windows-Teile hinter `cfg(windows)`.
  - `src-tauri/src/update.rs`, `commands.rs`, `lib.rs`, `build.rs`,
    `installer/firewall-hooks.nsh`, `Cargo.toml`.
  - Frontend: `components/UpdateBanner.tsx`, `pages/MaintenancePage.tsx`,
    `App.tsx`, `types.ts`, `api.ts`, neu `src/io/updateBanner.mjs`.
  - CI: neu `.github/workflows/seiten-paket.yml`,
    `scripts/build-seiten-paket.mjs`; `release.yml`, `ci.yml`; neu
    `release/datenformat.json`.
- **Architekturregeln:**
  - R1: Alle neuen Aktionen sind Tauri-Commands (`seiten_uebernehmen`,
    `seiten_zurueck`, `kern_wechseln`, `kern_zurueck`, `update_meldung_weg`);
    das Frontend liest nur `update_info`. Der Ablauf lebt im Kern (ADR 0057).
  - R2, R5: unberührt. Kein Ergebnisweg ändert sich.
  - R3: Weg 1 wirkt nur auf den LAN-Pfad dieses PCs; Cloud bleibt
    Relay-Deploy. Weg 2 betrifft beide Pfade gleich.
  - R4: Beim Kernwechsel schließt die alte App die Relay-Verbindung, **bevor**
    die neue startet; es gibt nie zwei Hosts im Namespace.
  - R6: unberührt. Abrufe von Paket und Kern tragen keine `install_id`.
- **Konfiguration & Abwärtskompatibilität:**
  - Keine neuen Felder in `config.rs`.
  - Neue Dateien: im App-Datenverzeichnis `seiten/` (aktiv, vorher,
    `zustand.json`), `uebergabe.json`, `rueckweg.json`; im
    Installationsverzeichnis zeitweise `bts-light.neu.exe`,
    `bts-light.vorher.exe`, `bts-light.defekt.exe`.
  - `latest.json` bleibt byte-kompatibel; alle Zusatzangaben stehen in
    eigenen Dateien. `identifier` `de.badhub.btslight` und Updater-Pfad
    `download/bts-light/` bleiben.
  - Der erste Schritt auf eine Version mit diesem Feature läuft zwingend über
    den bisherigen Installer.
- **Datenschutz:** kein Personenbezug. Paket und Kern werden anonym geladen;
  die Log-Zeilen nennen Version, Folge und Ablehnungsgrund, keine Inhalte.
- **Abhängigkeiten:**
  - badhub-Server: nur neue Dateien unter `download/bts-light/` (`seiten.json`,
    `seiten/`, `kern.json`, `kern/`), hochgeladen wie `latest.json` per rsync
    ohne `--delete`. Kein Server-Code.
  - GitHub: Environment `seiten-hotfix` mit Pflicht-Freigabe und den beiden
    Signier-Secrets; Schreibrecht auf `hotfix/v*`.
  - Eine neue **direkte** Cargo-Dependency: `minisign-verify`. Sie liegt über
    `tauri-plugin-updater` bereits im `Cargo.lock`, bringt also keine neue
    Lieferkette. Alles andere ist vorhanden (`ring`, `flate2`, `base64`,
    `reqwest`, `windows`).

## Formate

### Seiten-Paket

Eine Datei `seiten-<kern>-<folge>.btsp`: gzip-komprimiertes JSON, als Ganzes
minisign-signiert (`.sig` daneben, derselbe Schlüssel wie der Updater).

```json
{
  "format": 1,
  "art": "seiten",
  "kern": "0.9.421",
  "folge": 2,
  "commit": "<sha>",
  "gebaut": "2026-11-16T14:03:00Z",
  "seiten": { "tablet.html": "…", "tl.html": "…" }
}
```

Version, Bindung und Inhalte liegen unter **einer** Signatur. Die App prüft in
dieser Reihenfolge und lehnt beim ersten Fehler ab:

1. Dateigröße ≤ 8 MB.
2. Signatur über die rohen Bytes — **vor** dem Entpacken.
3. Entpacken mit Deckel 32 MB.
4. `format == 1` und `art == "seiten"`.
5. Die Menge der Seitennamen ist exakt die der eingebauten Seiten. Namen
   werden nur gegen diese Liste verglichen, nie als Dateipfad benutzt.
6. Keine Seite ist leer, und jede enthält ihre Pflicht-Platzhalter (z. B.
   `__SEITEN_MARKE__` in `tablet.html`).
7. `kern` == eigene Version.
8. `folge` > aktive Folge, und die Folge ist nicht gesperrt.

### Manifeste auf dem Server

| Datei | Inhalt | Signiert | Schreiber |
|---|---|---|---|
| `seiten.json` | `{format:1, pakete:[{kern, folge}]}` — je Kern-Version die neueste Folge, höchstens die drei jüngsten Kern-Versionen | nein | Paket-Job |
| `seiten/seiten-<kern>-<folge>.btsp` + `.sig` | Paket | ja | Paket-Job |
| `kern.json` | `{format:1, version}` | nein | `release.yml` |
| `kern/kern-<version>.json` + `.sig` | Kern-Kopf | ja | `release.yml` |
| `kern/bts-light-<version>.exe` | Programmdatei | über den Kopf | `release.yml` |

Die unsignierten Indizes liefern nur Zahlen. Die Lade-Adressen baut die App
selbst aus einem festen Präfix. Hochgeladen wird erst die Nutzlast, dann der
Index.

### Kern-Kopf

```json
{
  "format": 1,
  "art": "kern",
  "version": "0.9.430",
  "exe_sha256": "<hex>",
  "exe_groesse": 23456789,
  "rueckwaerts_lesbar_ab": "0.9.425"
}
```

Signiert wird der Kopf, nicht die Programmdatei; die hängt per SHA-256 daran.
So gibt es keine Signatur über eine nackte exe, die sich einer Altversion über
ein gefälschtes `latest.json` als „Installer" unterschieben ließe. Die exe
wird im Release-Job aus dem fertigen Installer entnommen, damit sie mit der
installierten Datei identisch ist.

### `release/datenformat.json`

```json
{ "rueckwaerts_lesbar_ab": "0.9.425", "abdruck": "<hash>", "begruendung": "…" }
```

`rueckwaerts_lesbar_ab` ist die älteste Version, die die Daten der aktuellen
noch lesen kann. Der Release-Job kopiert den Wert in den Kern-Kopf. Ein
Wächter-Test bildet den `abdruck` aus den persistierten Formaten; siehe
ADR 0101.

## Ablauf Kernwechsel

A = alte App, W = Wächter (die alte Programmdatei mit `--waechter`, ohne
Fenster, ohne Tauri), N = neue App.

| # | Wer | Schritt | Frist |
|---|---|---|---|
| 0 | A | Vorladen: Kopf prüfen, exe laden, Größe und SHA-256 prüfen, als `bts-light.neu.exe` ins Installationsverzeichnis. **Probelauf** `bts-light.neu.exe --selbsttest` (endet vor dem Fensteraufbau mit Code 0). | 120 s, sonst kein Wechsel-Angebot (ursprünglich 10 s; gemessen wurden 15 s für den ersten Start unter Schutzsoftware, siehe ADR 0100) |
| 1 | A | Klick: **zuerst tauschen, solange die Übertragung noch läuft.** Alte `bts-light.vorher.exe` löschen; `bts-light.exe` → `bts-light.vorher.exe`; `bts-light.neu.exe` → `bts-light.exe`. Die laufende App stört das nicht — sie läuft aus der umbenannten Datei weiter. Scheitert ein Schritt: erste Umbenennung zurücknehmen, Fehler im Banner; die Übertragung wurde nie angehalten. | kann unter Schutzsoftware Sekunden dauern (gemessen bis 9,7 s) — **außerhalb der Lücke** |
| 2 | A | Wiederanlauf-Marker schreiben. | — |
| 3 | A | Übertragung anhalten wie `stop_sync` (Stände sichern, Ports frei, Relay-Verbindung zu). **Lücke beginnt.** | — |
| 4 | A | W aus `bts-light.vorher.exe` starten (losgelöst, mit stdin-Pipe, eigene Prozessnummer mitgeben), dann beenden. W schreibt `uebergabe.json` (`gestartet`) selbst, unmittelbar bevor er N startet. | — |
| 5 | W | Wartet auf das Ende von A (stdin-EOF **und** Ende des Prozesses), danach 0,4 s Nachlauf. | 20 s, dann A beenden |
| 6 | W | Startet N und hält dessen Prozess-Handle. | — |
| 7 | N | Start: Config laden, Marker nehmen, Übertragung im Kern starten. Nach dem Binden des Ports wird die Lücke geloggt. **Lücke endet.** | — |
| 8 | N | Meldet `angekommen`, sobald der Start durch ist **und** die Oberfläche den ersten Command gerufen hat. | — |
| 9a | W | Meldung da: W endet. `bts-light.vorher.exe` bleibt als Rückweg. | — |
| 9b | W | N endet vorher oder die Frist läuft ab: N beenden; `bts-light.exe` → `bts-light.defekt.exe`; `bts-light.vorher.exe` → `bts-light.exe`; Zustand `zurueckgefallen` mit Grund; Marker neu schreiben; alte App starten. | Startwert 15 s, nach Messung (PR 7a) festzulegen |

> **Geändert am 16.11.2026 nach der Messung (ADR 0100):** Der Tausch stand
> ursprünglich **nach** dem Anhalten der Übertragung, also in der Lücke. Auf
> einem Rechner mit Unternehmens-Schutzsoftware blockierte ein einzelnes
> Umbenennen bis zu 9,7 s. Der Tausch braucht die angehaltene Übertragung
> nicht — er steht deshalb jetzt **vor** dem Anhalten und kostet keine Lücke
> mehr. Nebenbei entfällt der Fall „Übertragung nach gescheitertem Tausch
> wieder starten". Die Frist des Wächters zählt ab dem gelungenen Start von
> N, nicht ab dem Startversuch.

Der **Kern-Rückweg** ist derselbe Ablauf mit vertauschten Rollen (`vorher`
wird wieder aktiv, die zurückgenommene Version wird die neue `vorher` und
gesperrt), einschließlich Wächter.

**Wann der Tausch überhaupt angeboten wird:** Windows, Installationsverzeichnis
beschreibbar (Schreibtest), Probelauf bestanden. Sonst zeigt das Banner den
bisherigen Installer-Weg.

**Aufräumen beim Start** (Fehler nur loggen): alte Installer in `%TEMP%`
(`bts-light-update-*-setup.exe`), `bts-light.neu.exe`, `*.defekt.exe`;
`bts-light.vorher.exe` nur behalten, wenn `rueckweg.json` zur laufenden Version
passt (ein Installer-Lauf dazwischen macht sie ungültig); in `seiten/` nur
aktiv und vorher.

## Banner und Wartungsseite

| Lage | Text (Kern der Aussage) | Knöpfe |
|---|---|---|
| Seiten-Paket passt, LAN | „Verbesserung für Tablets und Anzeigen liegt bereit – ohne Unterbrechung." | Jetzt übernehmen |
| dito, LAN + Cloud | Zusatz: „Gilt für Geräte im Hallennetz. Geräte über die Cloud bekommen ihre Seiten vom Server." | Jetzt übernehmen |
| nur Cloud | kein Seiten-Angebot; Wartungsseite: „Im Cloud-Betrieb kommen die Seiten vom Server." | — |
| Slave-Verbund | Zusatz: „Weitere PCs (andere Halle) werden dort einzeln aktualisiert." | — |
| Seiten übernommen | „Seiten aktualisiert. Tablets laden neu, sobald kein Spiel läuft – sonst zeigt das Tablet einen Hinweis. Monitore über ‚Neu laden'. Die Turnierleitungs-Seite bitte von Hand neu laden." | Zurück zum vorigen Stand · Schließen |
| Kern bereit, Tausch möglich | „Update vX liegt bereit. Auf N Feldern laufen Spiele – der Wechsel unterbricht wenige Sekunden, die Übertragung läuft von selbst weiter." | Jetzt wechseln · Beim Beenden einbauen |
| dito, neue Version erlaubt keinen Rückweg | Zusatz: „Ein Zurück zu vY ist nach diesem Update nicht möglich." | wie oben |
| Kern bereit, Tausch nicht möglich | bisheriger Text des Installer-Wegs | Jetzt neu starten · Beim Beenden einbauen |
| Kern-Update und Seiten-Paket zugleich | Seiten-Zeile zuerst, Kern-Zeile darunter | beide Knopfgruppen |
| Nach Wechsel, Rückweg erlaubt | „bts-light läuft jetzt mit vX." | Zurück zu vY · Schließen |
| Nach Wechsel, Rückweg gesperrt | „Zurück zu vY ist nicht möglich: vX speichert Daten in einer Form, die vY nicht lesen kann." | Knopf aus |
| Start-Wächter hat zurückgesetzt | „vX ließ sich nicht starten. bts-light läuft wieder mit vY, die Übertragung läuft." | Verstanden |
| Nach Rückweg | „vX wurde zurückgenommen und wird nicht erneut angeboten." | Schließen |
| Paket abgelehnt | kein Banner; Wartungsseite: „Paket abgelehnt, nichts geändert" mit Grund | — |

Die Wartungsseite zeigt dauerhaft „App vX · Seiten-Stand N" (N = 0 heißt
eingebaut) und die Rückweg-Knöpfe. Die Zuordnung Lage → Text → Knöpfe liegt
als reine Funktion in `src/io/updateBanner.mjs`.

## Akzeptanzkriterien

Weg 1 — Seiten:

- [ ] Nach „Jetzt übernehmen" bei laufender Übertragung ist die Zahl der
      offenen Tablet-, Monitor- und TL-Sockets unverändert; BTP- und
      Relay-Sitzung bestehen weiter.
- [ ] Ein Tablet ohne laufendes Spiel lädt binnen zweier Ping-Takte neu; ein
      Tablet im Spiel zeigt nur den vorhandenen Hinweis und zählt weiter.
- [ ] Seite und Marke wechseln gemeinsam: Kein Abruf liefert die neue Seite
      mit alter Marke oder umgekehrt.
- [ ] Nach einem App-Neustart gilt der übernommene Stand mit derselben Marke.
- [ ] Nach einem Kern-Update gilt der eingebaute Stand der neuen Version; das
      alte Paket wird verworfen.
- [ ] Ein Slave-Träger liefert nach dem Tausch die neuen Seiten aus; ein
      nicht eingespielter Slave behält seine Seiten ohne Neulade-Schleife.
- [ ] „Zurück zum vorigen Stand" stellt ohne Lücke den vorigen Stand her; die
      zurückgenommene Folge wird nicht mehr angeboten, eine höhere schon.
- [ ] Abgelehnt werden, jeweils mit Log-Zeile und unverändertem aktivem
      Stand: fehlende Signatur, veränderte Bytes, fremder Schlüssel, andere
      Kern-Version, gleiche oder niedrigere Folge, gesperrte Folge, fehlende
      oder zusätzliche Seite, fehlender Pflicht-Platzhalter, falsche `art`,
      Überschreitung eines Größendeckels.
- [ ] Netz weg beim Prüfen oder Laden: kein Fehlerbanner; ein schon
      bereitliegendes Angebot bleibt.
- [ ] Platte voll beim Ablegen: nichts wird aktiviert, keine halbe Datei
      bleibt als „aktiv" liegen.
- [ ] Ein beschädigtes Paket auf der Platte führt beim Start zum eingebauten
      Stand und einer Log-Zeile, nicht zum Absturz.
- [ ] Im reinen Cloud-Betrieb erscheint kein Seiten-Angebot; bei LAN + Cloud
      steht der Reichweiten-Satz im Banner.

Paket-Job:

- [ ] Ein von Hand auf `main` gestarteter Lauf für `vX.Y.Z` erzeugt bei
      reiner HTML-Änderung auf `hotfix/vX.Y.Z` nach Freigabe ein Paket mit
      `kern` = X.Y.Z und der richtigen Folge.
- [ ] Ändert der Zweig einen Kern-Pfad gegenüber dem Tag, bricht der Job vor
      dem Signieren mit erklärender Meldung ab.
- [ ] Ohne Freigabe im Environment wird nichts signiert und nichts
      veröffentlicht.
- [ ] Kein Push startet den Job; der Hotfix-Zweig wird im Lauf nie
      ausgecheckt oder ausgeführt.
- [ ] Eine verschobene Kern-Datei, ein Pfad mit Sonderzeichen und eine Seite,
      die ein Verweis ist, führen jeweils zum Abbruch.
- [ ] Liegt unter dem Paketnamen schon ein anderer Inhalt auf dem Server,
      bricht der Lauf ab, statt zu überschreiben oder zu überspringen.
- [ ] Ein vom Bau-Skript erzeugtes Paket besteht die Prüfung der App.

Weg 2 — Kern:

- [ ] Die App loggt je Wechsel die Lücke in Millisekunden; Erfolgskriterium 2
      ist am Referenz-PC erfüllt und im Feldtest-Protokoll festgehalten.
- [ ] Nach dem Wechsel läuft die Übertragung ohne Klick; Feldstempel und
      Automatik-Pause sind erhalten.
- [ ] Zu keinem Zeitpunkt sind zwei Hosts im Relay-Namespace; die Monitore im
      Cloud-Betrieb zeigen während des Wechsels keine Offline-Blende.
- [ ] Schlägt der Probelauf fehl (z. B. Virenscanner sperrt die Datei), wird
      der Tausch nicht angeboten; es entsteht keine Lücke.
- [ ] Scheitert die zweite Umbenennung, läuft die alte Version mit
      Übertragung weiter und das Banner nennt den Fehler.
- [ ] Endet die neue Version vor ihrer Ankunftsmeldung oder läuft die Frist
      ab, läuft danach die alte Version mit Übertragung, zeigt die Meldung
      des Start-Wächters und bietet die Version nicht erneut an.
- [ ] Kopf mit falscher Signatur, Kopf für eine nicht neuere Version, exe mit
      abweichendem Hash oder abweichender Größe, Datei ohne `MZ`: jeweils
      abgelehnt, kein Angebot.
- [ ] Ist das Installationsverzeichnis nicht beschreibbar oder läuft die App
      nicht unter Windows, bietet das Banner den bisherigen Installer-Weg.
- [ ] „Beim Beenden einbauen" verhält sich wie bisher und hinterlässt eine
      gültige Vorversion.

Rückweg Kern:

- [ ] „Zurück zu vY" ist nur aktiv, wenn vY ≥ `rueckwaerts_lesbar_ab` der
      laufenden Version; sonst ist der Knopf aus und nennt den Grund. Fehlt
      die Angabe, gilt „nicht lesbar".
- [ ] Erlaubt die angebotene Version keinen Rückweg zur laufenden, steht das
      schon im Angebot, vor dem Klick auf „Jetzt wechseln".
- [ ] Nach dem Rückweg läuft vY mit Übertragung; vX wird nicht erneut
      angeboten, eine neuere Version schon.
- [ ] Eine `bts-light.vorher.exe`, die nicht zur laufenden Version gehört,
      wird beim Start verworfen; der Knopf erscheint nicht.
- [ ] Ändert ein PR ein persistiertes Format, ohne `release/datenformat.json`
      anzufassen, ist die CI rot.

Übergreifend:

- [ ] Eine Installation auf v0.9.421 aktualisiert über `latest.json` wie
      bisher; die neuen Dateien auf dem Server stören sie nicht.
- [ ] Die Deinstallation hinterlässt keine `bts-light.*.exe`.

## Tests

Rust-Unit-Tests (TDD, je PR zuerst):

- **Seitenstand** (`tablet/assets.rs`):
  `alle_html_dateien_im_assets_ordner_sind_tauschbar`,
  `eingebauter_stand_traegt_die_marke_des_tablet_html`,
  `tausch_wechselt_seite_und_marke_gemeinsam`,
  `pong_nennt_nach_tausch_die_neue_marke`,
  `pflicht_platzhalter_stehen_in_jeder_eingebauten_seite`,
  `demoseite_nutzt_denselben_schnappschuss`.
- **Paket** (`seiten_paket.rs`, mit Testschlüssel):
  `gueltiges_paket_wird_angenommen`, `unsigniertes_paket_abgelehnt`,
  `veraenderte_bytes_abgelehnt`, `fremder_schluessel_abgelehnt`,
  `andere_kern_version_abgelehnt`, `aeltere_folge_abgelehnt`,
  `gleiche_folge_abgelehnt`, `gesperrte_folge_abgelehnt`,
  `fehlende_seite_abgelehnt`, `fremde_seite_abgelehnt`,
  `fehlender_platzhalter_abgelehnt`, `gzip_bombe_abgelehnt`,
  `falsche_art_abgelehnt`,
  `kaputtes_paket_auf_platte_faellt_auf_eingebaut_zurueck`,
  `paket_fuer_alte_kern_version_wird_nach_update_verworfen`,
  `schreibfehler_laesst_aktiven_stand_unberuehrt`,
  `paket_aus_dem_bauskript_besteht_die_app_pruefung`.
- **Angebot und Rückweg Seiten** (`update.rs`):
  `angebot_nur_bei_gleicher_kern_version`, `netz_weg_laesst_bereites_paket`,
  `uebernehmen_aktiviert_und_merkt_vorher`, `zurueck_sperrt_die_folge`,
  `gesperrte_folge_nicht_angeboten_hoehere_schon`,
  `reine_cloud_bietet_nichts_an`, `slave_modus_bietet_an`.
- **Wiederanlauf im Kern** (`commands.rs`/`lib.rs`):
  `kern_wiederanlauf_startet_bei_frischem_marker`,
  `kein_wiederanlauf_ohne_startbedingungen_und_grund_wird_gemeldet`,
  `marker_wird_genau_einmal_genommen`,
  `pause_der_automatik_ueberlebt_den_wiederanlauf`,
  `luecke_wird_aus_stop_und_bind_berechnet`.
- **Kern-Kopf und Datenformat** (`uebergabe.rs`):
  `kopf_mit_falscher_signatur_abgelehnt`, `exe_hash_passt_nicht_abgelehnt`,
  `exe_groesse_passt_nicht_abgelehnt`, `kopf_fuer_aeltere_version_abgelehnt`,
  `kein_mz_abgelehnt`, `abdruck_weicht_ab_macht_rot`,
  `fehlende_angabe_heisst_nicht_lesbar`.
- **Tausch und Wächter** (`uebergabe.rs`, Umbenennen im Temp-Verzeichnis, auf
  Linux lauffähig): `tausch_plan_reihenfolge`,
  `zweites_umbenennen_scheitert_rollt_zurueck`,
  `kein_schreibrecht_waehlt_installer_weg`,
  `probelauf_gescheitert_kein_tausch_angebot`,
  `waechter_erfolg_bei_ankunft`, `waechter_rueckfall_bei_kind_ende`,
  `waechter_rueckfall_bei_frist`,
  `rueckfall_schreibt_frischen_marker_und_grund`,
  `zurueckgefallene_version_gesperrt`, `nicht_windows_meldet_grund`.
- **Rückweg Kern und Aufräumen**:
  `rueckweg_nur_wenn_vorher_version_lesbar`, `rueckweg_knopf_aus_mit_grund`,
  `veraltete_vorher_exe_wird_verworfen`,
  `zurueckgenommene_version_nicht_erneut_angeboten`,
  `aufraeumen_laesst_aktiv_und_vorher`, `temp_installer_werden_entfernt`.

Node-Tests (in `ci.yml`):

- `scripts/test-seiten-paket.mjs`: Kern-Pfad-Regel (Kern-Pfad geändert → kein
  Paket; nur HTML → Paket mit Tag-Version), Folge-Berechnung, Seitenliste des
  Bau-Skripts gegen die Liste in `assets.rs` (Drift), Index hält höchstens
  drei Kern-Versionen.
- `scripts/test-update-banner.mjs`: jede Zeile der Banner-Tabelle.

`cargo test`, `cargo clippy --workspace --all-targets -- -D warnings` und
`npm run build` grün je PR.

Messung und Feldtest (nur am Windows-PC):

- `tests/uebergabe_probe.rs` (ignoriert, von Hand): laufende exe umbenennbar,
  Installationsverzeichnis beschreibbar, Dauer des Virenscans beim ersten
  Start, Wächter überlebt das Ende der App, Hash der installierten exe gleich
  dem Artefakt, Reste nach der Deinstallation, Kaltstartzeit. Das Ergebnis
  legt die Wächter-Frist fest und steht in ADR 0100, **bevor** der Tausch
  scharf geschaltet wird.
- Feldtest: zehn Wechsel und zehn Installer-Läufe mit geloggter Lücke; ein
  Wechsel unter Last (viele Tablets verbunden); Cloud-Betrieb (Host-Slot,
  keine Offline-Blende); ein erzwungener Rückfall; ein Seiten-Tausch mit
  Tablet im Spiel.

## Risiken & Rollback

- **Signaturschlüssel außerhalb des Tag-Wegs.** Mit dem Paket-Job entsteht
  signierter Code ohne Admin-Tag. Schranken: Handstart nur von `main`,
  Pflicht-Freigabe, Abbruch bei Kern-Diff, Inhalte entstehen ausschließlich
  aus dem Repo.
- **Ein Seiten-Paket ändert das Verhalten aller Geräte dieses PCs auf
  einmal.** Entschärfung: Rückweg ohne Lücke.
- **Wächter-Frist.** Zu kurz: falscher Rückfall auf langsamen PCs. Zu lang:
  im Fehlerfall wächst die Lücke. Deshalb erst messen.
- **Teilzustand.** Fällt der Strom zwischen den zwei Umbenennungen aus, fehlt
  `bts-light.exe`. Das Fenster ist Millisekunden breit; heilen muss es eine
  Neuinstallation. Wird dokumentiert.
- **Virenscanner und SmartScreen.** Die Programmdatei ist nicht
  code-signiert. Der Probelauf fängt eine Sperre ab, bevor eine Lücke
  entsteht; das Verhalten zeigt nur der Feldtest.
- **Rückweg verliert neue Einstellungen.** Auch bei „lesbar" wirft die
  Vorversion Felder, die sie nicht kennt, beim nächsten Speichern weg. Das
  gehört zur Definition von „lesbar" (ADR 0101) und steht im Handbuch.
- **Hotfix nur auf dem Zweig.** Wird der Fix nicht auch nach `main` gemergt,
  fehlt er Cloud-Geräten und geht mit dem nächsten Release verloren. Der
  Paket-Job schreibt eine Erinnerung in seine Zusammenfassung; `release.md`
  nennt den Ablauf.
- **Rollback des Features selbst:** Weg 1 ist nach PR 4 abgeschlossen und
  einzeln nutzbar. Weg 2 fällt bei jedem Zweifel (kein Schreibrecht,
  Probelauf, Plattform) auf den bisherigen Installer-Weg zurück; der bleibt
  vollständig erhalten.

### Bekannte Grenzen

- Ausgangslücke und Ziel sind bei Freigabe dieser Spec ungemessen. PR 5
  liefert die Ausgangszahl, PR 7a die Zielzahl.
- Ein Prozesswechsel verliert weiterhin: Ansage-Aufträge der letzten 60 s
  (`AnsageStore`), den Vertonungs-Zwischenspeicher (`SeitenCache`), Merker der
  Sync-Engine.
- Die Versionsanzeige unter „Apps & Features" bleibt nach einem Tausch alt,
  bis der nächste Installer läuft.
- TL-Web, Meetingpoint, Ansage und die übrigen Seiten ohne Versionsabgleich
  behalten den alten Stand bis zum Neuladen von Hand; Tablets im
  Energiesparmodus bis zum Aufwachen.

## Offene Fragen / Annahmen

Annahmen, die PR 7a am Windows-PC prüft; trifft eine nicht zu, bleibt Weg 2
beim Installer und die Spec wird nachgezogen:

- Eine laufende Programmdatei lässt sich im per-user-Installationsverzeichnis
  umbenennen.
- Der Wächter-Prozess überlebt das Beenden der App und erkennt es am Ende der
  stdin-Pipe.
- Der Deinstallierer löscht nur ihm bekannte Dateien; die Hilfsdateien
  entfernt ein Eintrag im vorhandenen Uninstall-Hook.

Annahme für PR 3: `npx tauri signer sign` läuft auf dem Linux-Runner ohne
Rust-Build. Trifft das nicht zu, signiert der Job mit einem anderen
minisign-Werkzeug; das Format bleibt.

Keine offenen fachlichen Fragen.

## Betroffene Doku-Dateien

- `docs/release.md` — Hotfix-Zweig, Paket-Job, Environment und Secrets,
  Kern-Artefakt, `datenformat.json`.
- `docs/features/update-im-turnierbetrieb.md` — Wiederanlauf im Kern,
  Banner-Tabelle, Verweis hierher.
- `docs/features/tablet-version-abgleich.md`, `docs/tablet.md` — Marke
  wechselt zur Laufzeit.
- `docs/oberflaechen.md` — Banner und Wartungsseite, mit `pruef`-Ankern auf
  die Beschriftungen.
- `docs/erste-schritte.md` — Update-Ablauf aus Sicht der Turnierleitung.
- `docs/multi-hall.md`, `docs/master-slave.md` — jeder PC für sich.
- `docs/cloud-relay.md` — Reichweite: Cloud-Seiten kommen vom Relay.
- `docs/logging.md` — neue Log-Zeilen (Lücke, Ablehnungsgründe, Rückfall).
- `docs/roadmap.md`, `docs/changelog.md` je Version.
- `CLAUDE.md` — neue Zeile in der Doku-Tabelle.
- ADR 0099–0101; ADR 0057 erhält einen Verweis.

## Umsetzungs-Hinweise

Neun kleine PRs. `code-reviewer` nach jedem; `security-reviewer` wo vermerkt.
Versionsnummer je PR erst beim Merge festlegen und in `src-tauri/Cargo.toml`,
`src-tauri/tauri.conf.json` und `package.json` gemeinsam setzen.

**Weg 1 (ohne Windows vollständig testbar):**

1. **Seiten-Quelle austauschbar** — reiner Umbau. `enum Seite` mit `ALLE`,
   `datei()`, `pflicht_platzhalter()`; `Seitenstand { folge, quelle, seiten,
   marke }` hinter `static RwLock<Arc<Seitenstand>>`; Zugriff über
   `assets::aktiv()`. Die `pub const` entfallen, damit jeder vergessene
   Aufrufer ein Compilerfehler ist. Seite und Marke kommen je Abruf aus
   **einem** Schnappschuss.
2. **Paket: Format, Prüfung, Ablage, Laden beim Start** — `seiten_paket.rs`,
   `signatur.rs`, `minisign-verify` direkt. Ablage in `app_data_dir/seiten/`,
   volle Prüfung bei jedem Start. *security-reviewer.*
3. **Paket-Job** — `scripts/build-seiten-paket.mjs`, `seiten-paket.yml`
   (Handstart auf `main` mit Tag, Zweig nur als Daten, Kern-Diff-Abbruch,
   Environment `seiten-hotfix`, Upload erst Nutzlast, dann Index aus der
   Server-Liste). *security-reviewer* (Secrets außerhalb
   des Release-Laufs).
4. **Angebot, Banner, Seiten-Rückweg** — `update_check` liest `seiten.json`,
   Commands `seiten_uebernehmen`/`seiten_zurueck`, `updateBanner.mjs`,
   Wartungsseite. Handbuch-Kapitel im selben PR.

**Weg 2:**

5. **Wiederanlauf im Kern und Lücken-Messung** — `start_sync` in einen Kern
   ohne `State` zerlegen, Aufruf in `.setup()`, Signal „Port gebunden", ein
   gescheitertes Binden geht in den Status statt nur ins Log. Regressionstest
   für die Automatik-Pause. Nützt auch dem Installer-Weg und liefert dessen
   Messwert.
6. **Kern-Artefakt und Datenformat-Wächter** — `release.yml` entnimmt die exe
   dem Installer, schreibt und signiert den Kopf, veröffentlicht `kern.json`
   und `kern/`. `release/datenformat.json` mit Abdruck-Test.
   *security-reviewer.*
7. a) **Messwerkzeug** — `--selbsttest`, Wächter-Modus und Tausch hinter
   `tests/uebergabe_probe.rs`, ohne Anschluss ans Banner. Ergebnis in
   ADR 0100.
   b) **Kernwechsel scharf** — Vorladen, Probelauf, Tausch, Wächter, Banner;
   Uninstall-Hook. *security-reviewer* (Prozessstart, Dateitausch).
8. **Kern-Rückweg, Sperre, Aufräumen** — Vorversion auch vor dem
   Installer-Lauf sichern, `rueckweg.json`, Sperrliste, Aufräumregel.
   *security-reviewer.*
9. **Abschluss** — Feldtest-Protokoll, Handbuch, CLAUDE.md-Zeile, Roadmap.

Abhängigkeiten: 1 → 2 → 3 → 4; unabhängig davon 5 → 6 → 7a → 7b → 8.
Reihenfolge-Empfehlung: Weg 1 zuerst; er ist allein nützlich und trägt kein
Prozessrisiko.

Vor dem ersten Hotfix einzurichten (Admin): Environment `seiten-hotfix` mit
Freigabe-Berechtigten und den Secrets `TAURI_SIGNING_PRIVATE_KEY` und
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; Schreibrecht auf `hotfix/v*`.
