# Roadmap & offene Punkte

Lebende Liste der offenen Arbeiten an bts-light. Erledigte Versionen stehen
im [changelog.md](changelog.md); hier steht, was **noch** ansteht.

> Stand: 2026-05-22, nach Release v0.9.3. In Arbeit: Mehr-Hallen-
> Unterstützung (siehe unten).

## In Arbeit: Mehr-Hallen-Unterstützung (Standorte/Locations)

Turniere in mehreren Hallen sollen automatisch erkannt und in allen
Ansichten nach Halle getrennt werden. BTP liefert die Hallen (`Locations`)
und je Feld eine `LocationID` mit — bts-light liest das künftig aus
(siehe [btp_protocol.md](btp_protocol.md)). **Wichtig:** Feldnamen
wiederholen sich über Hallen hinweg (Halle 1 Feld 1 und Halle 2 Feld 1) —
die Feld-Identität wird daher von der Namens- auf die stabile
BTP-`CourtID` umgestellt. Umsetzung in 7 Schritten:

1. **BTP-Modell liest Locations** — `BtpLocation`/`BtpCourt`-Typen,
   `LocationID` + `SortOrder` je Feld, `BtpMatch.court_id`. Rein intern,
   gegen echten Zwei-Hallen-Mitschnitt getestet. ✅ erledigt
2. **Feld-Identität intern auf CourtID** — behebt die Verwechslung
   doppelter Feldnamen. ✅ erledigt
3. **Relay & Cloud-Pfad** — Wire-Typen + Relay auf CourtID. ✅ erledigt
4. **Tablet- & Monitor-Anzeige** — Court-Monitor zeigt „Halle 2 · Feld 6".
   ✅ erledigt
5. **Dashboard nach Hallen gruppiert** — Felder, Adressen, Geräte-Zuweisung.
   ✅ erledigt
6. **Hallen-Übersichts-Bildschirm** — entfällt als bts-light-eigene Seite:
   der badhub-Liveticker-Monitor (`/live?display=monitor`) übernimmt das,
   nach Hallen getrennt. bts-light sendet die Halle dafür im `tset`-Push
   mit (v0.9.8); die badhub-Seite (DB, `live.php`/`live.js`) folgt separat.
7. **Aufräumen** — Übergangs-Code (Namen-Fallback) entfernen.

Geräte-Hinweis: Bei Schritt 2 müssen Tablet-/Monitor-Kopplungen einmalig
neu zugewiesen werden (die alte Zuordnung hing am Feldnamen).

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

## Court-Monitor — offene Punkte

Der Court-Monitor ist umgesetzt (v0.7.0–v0.9.0, [court-monitor.md](court-monitor.md),
[pi-setup.md](pi-setup.md), [pi-master-image.md](pi-master-image.md)).
Offen für das **Verleih-Set**-Konzept (Technik wird an Turnierleitungen
verliehen):

- **mDNS noch ungeklärt — entscheidender Test offen.** Stand 2026-05-22:
  `bts-light.local` ließ sich von einem **Windows-PC** nicht auflösen
  (`ERR_NAME_NOT_RESOLVED`, auch nach Freigabe von UDP 5353 in der
  Windows-Firewall). Das ist **nicht aussagekräftig** — Windows ist als
  mDNS-*Client* selbst unzuverlässig; der Fehlschlag dort beweist nicht,
  dass bts-lights Bekanntgabe defekt ist. Der **entscheidende Test ist die
  Auflösung von einem Raspberry Pi** (avahi, das echte Court-Monitor-
  Gerät). Dieser Test ist **noch nicht möglich: die Court-Monitor-Pis sind
  noch nicht einsatzfähig** (kein Master-Image, kein eingerichteter Pi).
  Erst der Pi-Test entscheidet, ob an `mdns-sd` (Netzwerk-Adapter-Auswahl
  auf Windows) etwas zu fixen ist — oder ob mDNS für den echten Einsatzfall
  längst funktioniert. Bis dahin ist die **IP-Adresse der verlässliche
  Weg** (`http://<ip>:8088/…`, eingebauter Rückfall). *Alternative fürs
  Verleih-Set:* DHCP-Reservierung am vorkonfigurierten Verleih-Router →
  stabile IP ohne Laptop-Einstellung.
- **Master-Image erstellen + hosten.** Den „Golden Master"-Pi einmal auf
  echter Hardware bauen, die Karte als `bts-monitor.img.xz` sichern und in
  den Download-Bereich auf badhub.de legen. Ablauf: [pi-master-image.md](pi-master-image.md).
  **Abhängigkeit:** Welche Monitor-Adresse das Image fest einbäckt
  (`bts-light.local` vs. fixe Router-IP) hängt an der mDNS-Klärung oben —
  beim Festlegen müssen [pi-setup.md](pi-setup.md) **und**
  [pi-master-image.md](pi-master-image.md) mitgezogen werden.
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
