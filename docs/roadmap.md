# Roadmap & offene Punkte

Lebende Liste der offenen Arbeiten an bts-light. Erledigte Versionen stehen
im [changelog.md](changelog.md); hier steht, was **noch** ansteht.

> Stand: 2026-07-17, nach dem ersten Zwei-Hallen-Praxisturnier (v0.9.144).
> Die Prio-1-Punkte und Turnier-Wünsche stammen direkt aus diesem Einsatz.

## Prio 1 — Lehren aus dem Zwei-Hallen-Turnier (17.07.2026)

- **BTP-Ergebnis-Regression** *(Fix implementiert, wartet auf Release
  v0.9.145)*: Spiele wurden in BTP nicht automatisch beendet — `Status`
  fehlte seit v0.9.103 im Ergebnis-`SENDUPDATE`; zudem Ergebnis +
  Feldfreigabe jetzt in einem Request. **Vor dem Release am echten BTP
  gegenprüfen** (Spiel schließt automatisch; Aufgabe/Walkover; Check-in
  bei Feldzuweisung unverändert). Details: [btp_protocol.md](btp_protocol.md).
- **Master-Identität umziehen.** Ein Rechnertausch erzeugt eine neue
  `install_id` → alle gekoppelten Geräte (Slave, Pis, Tablets, TVs)
  verlieren still die Verbindung (Hauptursache des Turniertag-Chaos).
  Geführter Config-Export/-Import bzw. Identitäts-Übernahme im SetupWizard
  + Dashboard-Warnung, wenn bekannte Monitore länger offline sind.
- **Slave-PC als eingebaute Monitor-Brücke.** bts-light im Cloud-Slave-Modus
  soll selbst auf `:8088` lauschen (`/health` + Redirect `/monitor[?device=…]`
  auf den Cloud-Monitor des Masters) — dann laufen die Bestands-Pis
  (Tilos Image, Subnetz-Scan) in der fernen Halle ohne Zusatz-Skripte.
  Ersetzt die Turnier-Notlösung (`pi-bridge`-Skripte auf Mac/Windows).

## Turnier-Wünsche (17.07.2026)

- ~~**Court-Monitor: Spielernamen deutlich größer**~~ → umgesetzt in
  v0.9.145 (`assets/monitor.html`; Cloud-TVs per Relay-Deploy).
- ~~**Ansage nennt die Klasse** („Herreneinzel A")~~ → umgesetzt in
  v0.9.145 (`model::class_label`, Details [announcements.md](announcements.md)).
- **Spielübersicht für die Slave-Halle**: laufende/anstehende Spiele der
  eigenen Halle am Slave sehen (Datenquelle: Relay), nicht nur
  Geräte-Anschluss + Ansagen.
- **„Spiele in Vorbereitung" vom Slave (erneut) aufrufen**: Rückkanal über
  den Relay nötig — Sicherheitsmodell beachten (Slave ist bisher bewusst
  read-only, R4/R5).

## Turnier-Wünsche (18./19.07.2026 — zweites Wochenende)

Aus dem laufenden Betrieb notiert (Turnierleitung + Beobachtungen).
**Umsetzungspläne je Punkt:** [roadmap-plaene-2026-07.md](roadmap-plaene-2026-07.md).

- **Gezielter zweiter/dritter Aufruf — auch je Partei.** Ist ein Spiel
  aufgerufen, aber nur eine Seite erschienen, soll die Turnierleitung
  einen **zweiten Aufruf nur für die fehlende Partei** auslösen können
  (Ansage z. B. „Zweiter Aufruf für …"). Gewünscht auf dem **Master und
  vom Slave aus** — hängt am selben Relay-Rückkanal wie der
  Vorbereitungs-Aufruf vom Slave (siehe oben, R4/R5 beachten).
- **„Nächste Spiele pro Halle"** (Idee von Nik, Turnierleitung): BTP führt
  den **Spielort bereits an der Ansetzung** (Spalte „Spielort"/Feld, z. B.
  `WR-6`, `HM-05`) — diese Info kommt schon per `SENDTOURNAMENTINFO`.
  Daraus eine Aufruf-/Nächste-Spiele-Liste **je Halle** bauen.
  *Recherche 19.07.:* Der Hallen-Filter `&halle=…` existiert auf
  `badhub.de/live?display=next` **bereits** — es fehlt nur die
  senderseitige Hallen-Info an den angesetzten Spielen (bts-light,
  `planned_court_id` parsen). Details: [roadmap-plaene-2026-07.md](roadmap-plaene-2026-07.md).
- **Tablet: helles, akkuschonendes Styling.** Das dunkle Design zwingt die
  Schiedsrichter, die Display-Helligkeit hochzudrehen → Akkus leeren sich
  schneller. Ziel: helles Theme bzw. ein Kontrast-Styling, das auch bei
  **minimaler Helligkeit** klar ablesbar ist.
- **Tablet-Schrift größer** — ausdrücklich auch der **Spielstand** und die
  Texte allgemein (analog zur TV-Vergrößerung aus v0.9.145).
- **TV-Leerlauf: Feldnummer groß + badhub.de-Branding.** Ohne laufendes
  Spiel (z. B. Slave-Halle zwischen den Runden) soll der TV die Feldnummer
  prominent zeigen, darunter groß „badhub.de" (Orientierung + Werbung).
- **Spielstand direkt eintragen (Tablet + Turnierleitung).** Endstand
  eintippen, wenn niemand gezählt hat; Zwischenstand eintragen und ab da
  live weiterzählen, wenn ein Zähler verspätet einsteigt (nur Aufschläger,
  im Doppel Rückschläger, plus Satz nötig — Positionen folgen der
  BWF-Paritätsregel). Button offen sichtbar.
- **Klick-Delay am Tablet verkürzen.** Punkt soll bei Berührung zählen
  (`pointerdown` statt `click`), Persist/Sync raus aus dem Tap-Pfad.
- **Zähltafelbediener-Verwaltung** (wie Tilos BTS): Verlierer-Warteschlange,
  Zuweisung beim Feld-Aufruf, Mit-Ansage „Tabletbedienung: …",
  BTP-Auscheck, Mindestpause.
- **badhub `/live?tab=done`: Tages-Filter** (Wunsch 19.07.), initial auf
  den aktuellen Tag. Die Beendet-Einträge tragen bereits `end_ts` →
  reines Frontend im badhub-Repo (`live.js`): nach Tag gruppieren/filtern,
  kleines Tages-Dropdown. **Achtung Befund 19.07.:** Nach einem
  App-Neustart stempelt bts-light ALLE schon beendeten Spiele mit
  frischem `end_ts` → für den Tages-Filter Zeitquelle prüfen/festigen.
- **Beendet-Liste: Aufgabe/kampflos kennzeichnen** (Befund 19.07.).
  In BTP direkt gewertete Aufgaben erscheinen im Ticker als „beendet"
  mit Teil-Spielstand (z. B. 14:16, 15:10, 0:0) und wirken fehlerhaft.
  Fix: `score_status` (Aufgabe/Walkover) aus dem BTP-Snapshot in die
  `recent_finished`-Einträge des Payloads übernehmen (bts-light) und im
  Ticker als Badge „Aufgabe"/„kampflos" anzeigen (badhub `live.js`).
- *Nice-to-have:* **Zeit seit Aufruf** auf den TVs **und** in bts-light
  anzeigen (die Aufruf-Uhr existiert am Cloud-Monitor bereits als
  Datenquelle: `on_court_since`/Aufruf-Zeitstempel).
- *Nice-to-have:* **Pausenuhr als Overlay.** Die Pausenuhr auf den TVs ist
  gut — der Spielstand soll dabei aber sichtbar bleiben (Overlay statt
  Vollbild-Wechsel).
- **Analyse (badhub-Repo): Spielerprofil-Links auf `/live` teils defekt.**
  Die Links auf Spielerprofile funktionierten schon einmal; aktuell gehen
  einige, andere nicht — Ursache klären (Namens-Matching?).
- **BTP-Rückschreibung: Übernahmen aus Tilos Original-BTS** (Analyse
  19.07., [btp-write-vergleich-letilo.md](btp-write-vergleich-letilo.md)):
  Aufrufe als `Highlight` nach BTP melden, Retry-Queue für nicht
  bestätigte Ergebnisse, Disqualifikations-Code — Pläne in
  [roadmap-plaene-2026-07.md](roadmap-plaene-2026-07.md), Punkt 11.
- **HTTPS für den LAN-Tablet-Server — Akkustände auch im LAN sehen.**
  Browser geben die Battery-API (`navigator.getBattery`) nur in
  **sicheren Kontexten** frei: Cloud-Tablets (https via badhub.de) melden
  ihren Akkustand an die Felder-Übersicht, LAN-Tablets (`http://IP:8088`)
  können das prinzipbedingt nicht. Damit die Turnierleitung **alle**
  Tablet-Akkus sieht, braucht der eingebettete Server HTTPS (Optionen
  bewerten: eigenes lokales Zertifikat + Vertrauensstellung auf den
  Tablets vs. alles über den Cloud-Weg — Entscheidung als ADR).

## Nach dem Turnier-Wochenende (Stand 19.07.2026)

Gesammelte Nacharbeiten, sobald das Turnier vorbei ist:

- **Log-Review des Turnier-Wochenendes (geplant 20.07.2026):** Relay-Logs,
  hochgeladene App-Logs (Master + Slave, per `install_id`) und nginx-Log
  systematisch nach Auffälligkeiten durchgehen — insbesondere
  Tablet-Reconnects/State-Restores (Bug vom Samstag), Ergebnis-Übertragung
  nach BTP, Monitor-Ausfälle. **Live-Befunde vom 19.07. mitnehmen:**
  (a) BTP lieferte um 08:38:46 (und vermutlich 08:27) einen **leeren
  Snapshot** (0 Hallen/Felder/Matches) → Massen-Freigabe aller Felder,
  6 s später Wiederzuweisung; Abwehr prüfen (leeren Snapshot nach
  gefülltem erst nach 2. Bestätigung übernehmen + Warnung).
  (b) Ergebnis-Log-Zeile mit **leerem Hallen-Label** „Feld 38 ('')" —
  Label-Lookup schlug fehl, weil die Zuweisung schon aufgehoben war
  (kosmetisch). (c) HM-Tablets im Minutentakt „still >10s"-Zyklen —
  Bildschirm-/WLAN-Doze; Keep-Awake-Empfehlung in die Tablet-Doku.
- **Offizielles Release schnüren** (> 0.9.147, mit Auto-Update): Inhalte
  der TEST-Builds (BTP-Ergebnis-Fix, TV-Schrift, Klassen-Ansage,
  Slave-Brücke, 0.9.147 BTP-Felder + Tablet-Reconnect) plus der wartenden
  PRs #76 (Azure-TTS-Vererbung) und #78 (8-stelliger Kopplungscode).
  **Änderung am Kopplungscode: Gültigkeit 1 Stunde statt 15 Minuten.**
- **Server aufräumen:** nginx-Namespace-Rewrite (alte→neue Master-ID),
  Kurzlinks `wr1–6`/`wrtv1–6`, `pi-bridge-wr.ps1`, TEST-Exes im
  Download-Verzeichnis.
- **Azure-Speech-Key rotieren** (wurde während des Turniers im Klartext
  geteilt).
- **Pi-Kiosk-Image untersuchen:** Warum fahren frisch beschriebene Karten
  teils nicht hoch (Turnier-Befund; Tilos Image vs. unser Image).
- **Bug prüfen: Region-Feld am Slave nicht änderbar** (Azure-Ansagen).

## Mehr-Hallen-Unterstützung — Restposten

Die Mehr-Hallen-Architektur ist umgesetzt — CourtID-Identität, Hallen-
Gruppierung im UI, Liveticker-Hallen-Monitor (badhub), LAN+Cloud-
Parallelbetrieb (v0.9.4 – v0.9.13, Erzählung in
[multi-hall.md](multi-hall.md)). Geblieben ist ein technischer
Restposten:

- **Namens-Fallback entfernen.** Übergangs-Code, der Routing notfalls noch
  über den Feldnamen statt der CourtID erlaubt, kann nach mehreren stabilen
  Releases entfernt werden.

Geräte-Hinweis aus dem CourtID-Refactor: Tablet-/Monitor-Kopplungen mussten
einmalig neu zugewiesen werden (die alte Zuordnung hing am Feldnamen) —
gilt nur für Installationen, die schon vor v0.9.6 im Einsatz waren.

## Als Nächstes

- **Repo-Umbenennung** → Anzeigename „badhub BTP controller", GitHub-Repo
  `badhub-btp-controller`. Wichtig: Tauri-`identifier` `de.badhub.btslight`
  und der Updater-Pfad `download/bts-light/` bleiben **stabil**, sonst
  brechen bestehende Installationen beim Auto-Update. Der angezeigte
  `productName` kann separat und mit Bedacht wechseln.

## Geplant

- **Code-Signing des Windows-Installers.** Aktuell unsigniert → Windows
  zeigt beim ersten Start eine SmartScreen-Warnung. Optionen: Azure Trusted
  Signing vs. klassisches OV/EV-Zertifikat — Kostenentscheidung offen. Das
  Auto-Update ist davon unabhängig (eigenes Signaturschlüsselpaar).
- **CI-Wartung.** Die Release-/CI-Workflows nutzen Node-20-Actions
  (`actions/checkout@v4`, `actions/setup-node@v4`,
  `softprops/action-gh-release@v2`) — vor dem erzwungenen Node-24-Umstieg
  (ab 2026-06-02) aktualisieren. Außerdem leitet GitHub `windows-latest`
  ab 2026-06-15 auf `windows-2025` um — Build dort gegenprüfen.
- **Changelog pro Version sichtbar machen.** [`docs/changelog.md`](changelog.md)
  pflegt die Versionshistorie bereits, ist aber nirgends für Nutzer
  sichtbar — das Auto-Update zeigt aktuell nur „BTS Light X.Y.Z". Ziel:
  den Changelog-Eintrag der jeweiligen Version in die Update-Meldung
  (`latest.json` → `notes`) und in die GitHub-Release-Notes übernehmen,
  optional ein „Was ist neu"-Hinweis in der App. So sieht man, was sich
  von Version zu Version geändert hat.

## Feature-Wünsche

Von der Turnierleitung gewünscht, noch nicht eingeplant:

- **Aufgaben- & Walkover-Übersicht.** Eine Seite bzw. Kachel in bts-light,
  die während des Turniers alle Aufgaben und alle daraus gewerteten
  Walkovers auflistet — Überblick für die Turnierleitung.
- **Walkover zurücknehmen.** Eine kampflose Wertung wieder rückgängig
  machen können (Match in BTP zurück auf offen / `ScoreStatus = 0`),
  falls sie versehentlich oder falsch gesetzt wurde.
- **Tablet-Verbindungsanzeige im Cloud-Modus.** Schließt bts-light, bleibt
  das Tablet mit dem Relay verbunden und zeigt weiter „verbunden" — es
  erfährt nicht, dass der Host (bts-light) weg ist. Der Relay sollte den
  Tablets ein „Host offline"-Signal schicken, damit das Tablet ehrlich
  „Warte auf Turnier-PC" anzeigt.
- **Verbindungsweg je Gerät anzeigen (Parallelbetrieb).** Im
  LAN+Cloud-Modus pro verbundenem Gerät (Tablet, Court-Monitor) kenntlich
  machen, ob es bts-light über LAN oder über das Cloud-Relay erreicht —
  als Badge in der Felder-/Geräte-Übersicht. So sieht die Turnierleitung
  auf einen Blick, welchen Weg ein Gerät nutzt; hilft bei der Fehlersuche,
  wenn eine Halle hängt. (Der Relay/Server kennt den Weg ohnehin — er muss
  ihn nur je Gerät bis in die Übersicht durchreichen.)
- **Pausen-Buttons auf dem Tablet vereinheitlichen.** Die Buttons für
  Verletzungs-/Behandlungspause und der „Weiterspielen"-Button, mit dem
  eine laufende Pause beendet wird, sind uneinheitlich in Beschriftung,
  Größe und Anordnung. Über alle Pausen-Typen hinweg angleichen, damit
  die Bedienung im Spielbetrieb eindeutig ist.
- **Akkustand farblich kodieren (Tablet-Übersicht).** In der Felder-
  Übersicht soll der Tablet-Akkustand auf einen Blick zeigen, ob ein
  Tablet getauscht oder nachgeladen werden muss: **> 50 % grün**, **< 20 %
  rot**, dazwischen gelb. Schwellen am `TabletBattery.percent` in
  [`pages/TabletPanel.tsx`](../src/pages/TabletPanel.tsx); Ladezustand
  (`charging`) bleibt das bestehende Symbol.

## Court-Monitor — offene Punkte

Der Court-Monitor ist umgesetzt (v0.7.0–v0.9.0, [court-monitor.md](court-monitor.md),
[pi-setup.md](pi-setup.md), [pi-master-image.md](pi-master-image.md)).
Offen für das **Verleih-Set**-Konzept (Technik wird an Turnierleitungen
verliehen):

- **mDNS funktioniert auf Pi/avahi (verifiziert 2026-05-25).** Der seit
  Mai 2026 offene Entscheidungstest ist durchgeführt: ein Raspberry Pi mit
  Pi OS Lite (avahi-daemon) löst `bts-light.local` zuverlässig zu der IP
  des sendenden Geräts auf. Test-Setup: bts-light-Bekanntmachung
  (`_bts-light._tcp.local.` mit Hostname `bts-light.local.`, Port 8088)
  vom Mac aus per `dns-sd -P` simuliert → vom Pi aus mit
  `avahi-resolve -n bts-light.local` aufgelöst → IP korrekt empfangen,
  auch über die WLAN↔Ethernet-Bridge der FRITZ!Box hinweg. Damit ist die
  damalige Windows-PC-Fehlschlag-Beobachtung als reines
  Windows-mDNS-Client-Problem identifiziert; **bts-lights mDNS-Bekannt­
  machung in `tablet/mdns.rs` ist korrekt**. Konsequenz: das Master-Image
  bäckt `http://bts-light.local:8088/monitor` als Kiosk-Adresse ein, eine
  DHCP-Reservierung am Verleih-Router ist nicht notwendig (kann als
  Worst-Case-Rückfall jederzeit nachgezogen werden).
- **Master-Image erstellen + hosten.** Den „Golden Master"-Pi einmal auf
  echter Hardware bauen, die Karte als `bts-monitor.img.xz` sichern und in
  den Download-Bereich auf badhub.de legen. Ablauf: [pi-master-image.md](pi-master-image.md).
  Monitor-Adresse: **`http://bts-light.local:8088/monitor`** (durch den
  mDNS-Test oben bestätigt).
- **Hardware-Anforderung Pi Zero 2 W oder höher** (Hinweis 2026-05-25
  konkretisiert): Pi Zero W (1. Gen) und Pi Zero 2 W sehen physisch
  identisch aus, sind aber komplett verschiedene Chips. Pi Zero W (1. Gen,
  armv6 ARM1176JZF-S) hat **keine NEON-SIMD-Einheit**; modernes Chromium
  ist auf Debian Trixie / Pi OS Bookworm mit NEON als **Pflicht**
  kompiliert → Pi Zero W zeigt beim Start einen Hardware-Fehler-Dialog,
  ist als Court-Monitor **unbrauchbar**. Pi Zero 2 W (Cortex-A53), Pi 3,
  Pi 4 und Pi 5 haben alle NEON, dort läuft alles. 64-bit-Boot
  funktioniert nur ab Pi Zero 2 W (Symptom auf Pi Zero W: 7-Blink
  „kernel image not found"). Empfehlung für Verleih-Set-Hardware:
  Pi Zero 2 W (klein, günstig, ausreichend für den Kiosk) oder Pi 4
  (deutlich kraftvoller).
- **Info-Monitor: Routen + HTML ausgeliefert** (v0.9.17, 2026-05-25), **UI-
  Zuweisung offen.** Der Tablet-Server liefert jetzt zwei Hallen-Displays
  unter dedizierten URLs: `/info/overview` (Court-Übersicht, Hallen ×
  Felder × aktuelles Spiel) und `/info/preparation` (gerufene und
  eingeplante Spiele). Beide offline-fähig — Daten direkt aus
  `BtpSnapshot`, kein badhub.de nötig. URL-Parameter `?halle=<Name>` und
  `?rotate=90|180|270` unterstützt. Details
  [court-monitor.md → Info-Monitor](court-monitor.md). **Offen:**
  Zuweisung über die „Court-Monitore"-Seite (statt manuell die
  `bts-monitor-url.txt` zu bearbeiten) — Mock-Up des Dropdowns:
  ```
  Halle 1
    Feld 1
    Feld 2
  Halle 2
    Feld 1
    Feld 2
  Informationen
    Courtübersicht
    In Vorbereitung
  ```
  Setzt eine Erweiterung des `monitor_assignments`-Datenmodells voraus
  (Target = Court(i64) | InfoOverview | InfoPreparation) und ein
  zusätzliches Dropdown-Element im Frontend; der `/monitor`-Endpoint
  würde dann je Target-Typ die passende HTML zurückgeben.
- **Display-Rotation für Pivot-Monitore: URL-Parameter umgesetzt**
  (v0.9.17, 2026-05-25), **zentrale Steuerung offen.** `?rotate=90|180|270`
  am URL der Monitor-Seiten dreht die Anzeige per CSS-Transform — Pi-
  OS-seitig keine Änderung nötig. Das CSS rendert auch in Portrait
  sauber. **Offen:** Rotation als Geräte-Eigenschaft zentral aus bts-light
  pro Pi steuerbar (ohne `bts-monitor-url.txt` editieren zu müssen).
  Implementation: zusätzliches Feld `rotation: Option<u16>` in der
  Geräte-Zuweisung; bts-monitor.sh hängt `?rotate=…` an die URL an.
- **Online-Anleitung veröffentlichen.** [pi-setup.md](pi-setup.md) als
  echte Webseite (badhub.de) bereitstellen und **in bts-light verlinken**
  (Knopf „Einrichtungs-Anleitung" auf der Court-Monitore-Seite).
- **2-Felder-pro-TV-Modus.** Zwei benachbarte Felder auf einem großen TV
  (`…/display?courts=3,4`).

## Bekannte Einschränkungen / technische Schuld

- **Liga-Matches** (`PlayerMatches` in BTP) sind nicht abgedeckt — bts-light
  verarbeitet nur Einzel-/Doppel-Draws.
- **Spielsystem fest Best-of-3 bis 21.** BTP liefert das Spielformat im
  aktuellen Parser nicht zuverlässig; der Tablet-Spielzettel nimmt den
  Badminton-Normalfall an.
- **Liveticker-Staleness uneinheitlich.** Im `/live`-Picker fällt das
  „Live"-Badge nach 4 Min ohne Heartbeat weg, die Detailseite (`?t=`)
  zeigt „Nicht mehr live" erst nach 10 Min. Die 10-Min-Schwelle ist
  bewusst lose gehalten, solange Nicht-Heartbeat-Quellen (`letilo/bts`)
  pushen können. Angleichen, sobald bts-light die einzige Quelle ist.
- **Keine Frontend-Tests.** Der Rust-Kern ist per `cargo test` abgedeckt;
  die React-Seite (u. a. `announcer.ts` — Court-Phrase, Ansage-Segmente,
  Auto-Sprach-Regel) hat kein Test-Setup. badhub-tournament nutzt Vitest
  inkl. `announcer.test.ts` — das ließe sich übernehmen.
- **Alte Liveticker-Test-Turniere.** `lehiero`, `christian-zum-test` und
  die Legacy-Zeile `default` stehen in `liveticker_tournaments` noch auf
  `is_active = 1` und machen `/live` ohne `?t=` mehrdeutig. Im
  Liveticker-Admin auf inaktiv setzen.
- **`docs/ops/deployment.md` teils veraltet** (badhub-Repo): Der Abschnitt
  „Deploy: Produktion" beschreibt noch das KAS-`deploy_prod.sh`, obwohl
  Prod längst über `deploy_hetzner.sh` auf Hetzner läuft.
